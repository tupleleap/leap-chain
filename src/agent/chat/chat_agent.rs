use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;

use crate::{
    agent::{agent::Agent, chat::prompt::FORMAT_INSTRUCTIONS, AgentError},
    chain::chain_trait::Chain,
    message_formatter,
    prompt::{
        HumanMessagePromptTemplate, MessageFormatterStruct, MessageOrTemplate, PromptArgs,
        PromptFromatter,
    },
    prompt_args,
    schemas::{
        agent::{AgentAction, AgentEvent},
        messages::Message,
    },
    template_jinja2,
    tools::Tool,
};

use super::{output_parser::ChatOutputParser, prompt::TEMPLATE_TOOL_RESPONSE};

pub struct ConversationalAgent {
    pub(crate) chain: Box<dyn Chain>,
    pub(crate) tools: Vec<Arc<dyn Tool>>,
    pub(crate) output_parser: ChatOutputParser,
}

impl ConversationalAgent {
    pub fn create_prompt(
        tools: &[Arc<dyn Tool>],
        suffix: &str,
        prefix: &str,
    ) -> Result<MessageFormatterStruct, AgentError> {
        let tool_string = tools
            .iter()
            .map(|tool| format!("> {}: {}", tool.name(), tool.description()))
            .collect::<Vec<_>>()
            .join("\n");
        let tool_names = tools
            .iter()
            .map(|tool| tool.name())
            .collect::<Vec<_>>()
            .join(", ");

        let sufix_prompt = template_jinja2!(suffix, "tools", "format_instructions");

        let input_variables_fstring = prompt_args! {
            "tools" => tool_string,
            "format_instructions" => FORMAT_INSTRUCTIONS,
            "tool_names"=>tool_names
        };

        let sufix_prompt = sufix_prompt.format(input_variables_fstring)?;
        let formatter = message_formatter![
            MessageOrTemplate::Message(Message::new_system_message(prefix)),
            MessageOrTemplate::MessagesPlaceholder("chat_history".to_string()),
            MessageOrTemplate::Template(
                HumanMessagePromptTemplate::new(template_jinja2!(
                    &sufix_prompt.to_string(),
                    "input"
                ))
                .into()
            ),
            MessageOrTemplate::MessagesPlaceholder("agent_scratchpad".to_string()),
        ];
        return Ok(formatter);
    }

    fn construct_scratchpad(
        &self,
        intermediate_steps: &[(AgentAction, String)],
    ) -> Result<Vec<Message>, AgentError> {
        let mut thoughts: Vec<Message> = Vec::new();
        for (action, observation) in intermediate_steps.into_iter() {
            thoughts.push(Message::new_ai_message(&action.log));
            let tool_response = template_jinja2!(TEMPLATE_TOOL_RESPONSE, "observation")
                .format(prompt_args!("observation"=>observation))?;
            thoughts.push(Message::new_human_message(&tool_response));
        }
        Ok(thoughts)
    }
}

#[async_trait]
impl Agent for ConversationalAgent {
    async fn plan(
        &self,
        intermediate_steps: &[(AgentAction, String)],
        inputs: PromptArgs,
    ) -> Result<AgentEvent, AgentError> {
        let scratchpad = self.construct_scratchpad(&intermediate_steps)?;
        let mut inputs = inputs.clone();
        inputs.insert("agent_scratchpad".to_string(), json!(scratchpad));
        let output = self.chain.call(inputs.clone()).await?.generation;
        let parsed_output = self.output_parser.parse(&output)?;
        Ok(parsed_output)
    }

    fn get_tools(&self) -> Vec<Arc<dyn Tool>> {
        self.tools.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::{error::Error, sync::Arc};

    use async_trait::async_trait;
    use leap_connect::v1::api::Client as leap_client;
    use serde_json::Value;
    use simple_logger::SimpleLogger;

    use crate::llm::ollama::client::Ollama;
    use crate::llm::tupleleapai::client::Tupleleap;
    use crate::{
        agent::{chat::builder::ConversationalAgentBuilder, executor::AgentExecutor},
        chain::chain_trait::Chain,
        llm::openai::{OpenAI, OpenAIModel},
        memory::SimpleMemory,
        prompt_args,
        tools::Tool,
    };

    struct Calc {}

    #[async_trait]
    impl Tool for Calc {
        fn name(&self) -> String {
            "Calculator".to_string()
        }
        fn description(&self) -> String {
            "Usefull to make calculations".to_string()
        }
        async fn run(&self, _input: Value) -> Result<String, Box<dyn Error>> {
            Ok("25".to_string())
        }
    }

    #[tokio::test]
    async fn test_invoke_agent() {
        SimpleLogger::new().init().unwrap();
        // let client = leap_client::new(env::var("TUPLELEAP_AI_API_KEY").unwrap().to_string());
        // let llm = Tupleleap::new(client, "mistral".into());
        // let llm: OpenAI<async_openai::config::OpenAIConfig> = OpenAI::default().with_model(OpenAIModel::Gpt4.to_string());
        let llm = Ollama::default().with_model("llama3");
        let memory = SimpleMemory::new();
        let tool_calc = Calc {};
        let agent = ConversationalAgentBuilder::new()
            .tools(&[Arc::new(tool_calc)])
            .build(llm)
            .unwrap();
        let input_variables = prompt_args! {
            "input" => "hola,I am newbie and I am 10 years old. I am studying computer science",
        };
        let executor: AgentExecutor<crate::agent::ConversationalAgent> =
            AgentExecutor::from_agent(agent).with_memory(memory.into());
        match executor.invoke(input_variables).await {
            Ok(result) => {
                println!("Result A: {:?}", result);
            }
            Err(e) => panic!("Error invoking LLMChain: {:?}", e),
        }
        let input_variables = prompt_args! {
            "input" => "what is my age after 15 years",
        };
        match executor.invoke(input_variables).await {
            Ok(result) => {
                println!("Result B: {:?}", result);
            }
            Err(e) => panic!("Error invoking LLMChain: {:?}", e),
        }
    }
}
