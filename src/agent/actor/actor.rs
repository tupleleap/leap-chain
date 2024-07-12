use ractor::{async_trait, cast, Actor, ActorProcessingErr, ActorRef};

/// [PingPong] is a basic actor that will print
/// ping..pong.. repeatedly until some exit
/// condition is met (a counter hits 10). Then
/// it will exit
pub struct PingPong;

/// This is the types of message [PingPong] supports
#[derive(Debug, Clone)]
pub enum Message {
    Ping,
    Pong,
}

impl Message {
    // retrieve the next message in the sequence
    fn next(&self) -> Self {
        match self {
            Self::Ping => Self::Pong,
            Self::Pong => Self::Ping,
        }
    }
    // print out this message
    fn print(&self) {
        match self {
            Self::Ping => print!("ping.."),
            Self::Pong => print!("pong.."),
        }
    }
}

// the implementation of our actor's "logic"
#[async_trait]
impl Actor for PingPong {
    // An actor has a message type
    type Msg = Message;
    // and (optionally) internal state
    type State = u8;
    // Startup initialization args
    type Arguments = ();

    // Initially we need to create our state, and potentially
    // start some internal processing (by posting a message for
    // example)
    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        _: (),
    ) -> Result<Self::State, ActorProcessingErr> {
        // startup the event processing
        cast!(myself, Message::Ping)?;
        // create the initial state
        Ok(0u8)
    }

    // This is our main message handler
    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        if *state < 10u8 {
            message.print();
            cast!(myself, message.next())?;
            *state += 1;
        } else {
            println!();
            myself.stop(None);
            // don't send another message, rather stop the agent after 10 iterations
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{env, error::Error, sync::Arc};

    use crate::{
        agent::{AgentExecutor, ConversationalAgentBuilder, OpenAiToolAgentBuilder},
        chain::{options::ChainCallOptions, Chain},
        llm,
        memory::SimpleMemory,
        prompt_args,
        tools::{CommandExecutor, SerpApi, Tool},
    };
    use llm::ollama::client::Ollama;
    use ractor::{async_trait, Actor};
    use serde_json::Value;
    use simple_logger::SimpleLogger;

    use super::PingPong;

    struct Date {}
    #[async_trait]
    impl Tool for Date {
        fn name(&self) -> String {
            "Date".to_string()
        }
        fn description(&self) -> String {
            "Useful when you need to get the date,input is  a query".to_string()
        }
        async fn run(&self, _input: Value) -> Result<String, Box<dyn Error>> {
            Ok("25  of november of 2025".to_string())
        }
    }

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
    async fn test_agent_reference_main() {
        SimpleLogger::new().init().unwrap();

        let llm = Ollama::default().with_model("llama3");
        let memory = SimpleMemory::new();
        let tool_calc = Calc {};
        let agent: crate::agent::ConversationalAgent = ConversationalAgentBuilder::new()
            .tools(&[Arc::new(tool_calc)])
            .build(llm)
            .unwrap();
        let input_variables = prompt_args! {
            "input" => "hola,Me llamo luis, y tengo 10 anos, y estudio Computer scinence",
        };
        let executor = AgentExecutor::from_agent(agent).with_memory(memory.into());
        match executor.invoke(input_variables).await {
            Ok(result) => {
                println!("Result: {:?}", result);
            }
            Err(e) => panic!("Error invoking LLMChain: {:?}", e),
        }
        log::info!("==========================================88");
        // let input_variables = prompt_args! {
        //     "input" => "cuanta es la edad de luis +10 y que estudia",
        // };
        // match executor.invoke(input_variables).await {
        //     Ok(result) => {
        //         println!("Result: {:?}", result);
        //     }
        //     Err(e) => panic!("Error invoking LLMChain: {:?}", e),
        // }
    }
    #[tokio::test]
    async fn test_agent_reference_2() {
        SimpleLogger::new().init().unwrap();
        println!("print");
        log::info!("hello");
        let key = "RUST_LOG";
        match env::var(key) {
            Ok(val) => println!("{}: {:?}", key, val),
            Err(e) => println!("couldn't interpret {}: {}", key, e),
        }

        let llm = Ollama::default().with_model("llama3");
        let memory = SimpleMemory::new();
        let serpapi_tool = SerpApi::default();
        let tool_calc = Date {};
        let command_executor = CommandExecutor::default();
        let agent = OpenAiToolAgentBuilder::new()
            .tools(&[
                Arc::new(serpapi_tool),
                Arc::new(tool_calc),
                Arc::new(command_executor),
            ])
            .options(ChainCallOptions::new().with_max_tokens(1000))
            .build(llm)
            .unwrap();

        let executor = AgentExecutor::from_agent(agent).with_memory(memory.into());

        let input_variables = prompt_args! {
            "input" => "What the name of the current dir, And what date is today",
        };

        match executor.invoke(input_variables).await {
            Ok(result) => {
                println!("Result: {:?}", result.replace("\n", " "));
            }
            Err(e) => panic!("Error invoking LLMChain: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_actor1() {
        let (_actor, handle) = Actor::spawn(None, PingPong, ())
            .await
            .expect("Failed to start ping-pong actor");
        handle
            .await
            .expect("Ping-pong actor failed to exit properly");
    }
}
