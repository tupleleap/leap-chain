use leap_chain::llm::tupleleapai::client::Tupleleap;
use leap_chain::{
    chain::{Chain, LLMChainBuilder},
    llm::openai::{OpenAI, OpenAIModel},
    prompt::HumanMessagePromptTemplate,
    prompt_args, template_jinja2,
};
use leap_connect::v1::api::Client as leap_client;
use std::{
    env,
    io::{self, Write},
}; // Include io Library for terminal input

#[tokio::main]
async fn main() {
    let prompt = HumanMessagePromptTemplate::new(template_jinja2!(
        "Give me a creative name for a store that sells: {{producto}}",
        "producto"
    ));

    let client = leap_client::new(env::var("TUPLELEAP_AI_API_KEY").unwrap().to_string());
    let llm = Tupleleap::new(client, "mistral".into());
    let chain = LLMChainBuilder::new()
        .prompt(prompt)
        .llm(llm)
        .build()
        .unwrap();

    print!("Please enter a product: ");
    io::stdout().flush().unwrap(); // Display prompt to terminal

    let mut product = String::new();
    io::stdin().read_line(&mut product).unwrap(); // Get product from terminal input

    let product = product.trim();

    let output = chain
        .invoke(prompt_args!["producto" => product]) // Use product input here
        .await
        .unwrap();

    println!("Output: {}", output);
}
