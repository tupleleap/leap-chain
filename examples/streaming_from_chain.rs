use std::env;

use futures::StreamExt;
use leap_chain::llm::tupleleapai::client::Tupleleap;
use leap_chain::{
    chain::{Chain, LLMChainBuilder},
    fmt_message, fmt_template,
    llm::openai::OpenAI,
    message_formatter,
    prompt::HumanMessagePromptTemplate,
    prompt_args,
    schemas::messages::Message,
    template_fstring,
};
use leap_connect::v1::api::Client as leap_client;

#[tokio::main]
async fn main() {
    let client = leap_client::new(env::var("TUPLELEAP_AI_API_KEY").unwrap().to_string());
    let llm = Tupleleap::new(client, "mistral".into());

    let prompt = message_formatter![
        fmt_message!(Message::new_system_message(
            "You are world class technical documentation writer."
        )),
        fmt_template!(HumanMessagePromptTemplate::new(template_fstring!(
            "{input}", "input"
        )))
    ];

    let chain = LLMChainBuilder::new()
        .prompt(prompt)
        .llm(llm.clone())
        .build()
        .unwrap();

    let mut stream = chain
        .stream(prompt_args! {
        "input" => "Who is the writer of 20,000 Leagues Under the Sea?",
           })
        .await
        .unwrap();

    while let Some(result) = stream.next().await {
        match result {
            Ok(value) => value.to_stdout().unwrap(),
            Err(e) => panic!("Error invoking LLMChain: {:?}", e),
        }
    }
}
