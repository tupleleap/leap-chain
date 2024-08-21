use leap_chain::llm::tupleleapai::client::Tupleleap;
use leap_chain::{
    chain::{Chain, LLMChainBuilder},
    llm::openai::{OpenAI, OpenAIModel},
    prompt::HumanMessagePromptTemplate,
    prompt_args, sequential_chain, template_jinja2,
};
use leap_connect::v1::api::Client as leap_client;
use std::{
    env,
    io::{self, Write},
}; // Include io Library for terminal input
#[tokio::main]
async fn main() {
    let client = leap_client::new(env::var("TUPLELEAP_AI_API_KEY").unwrap().to_string());
    let llm = Tupleleap::new(client, "mistral".into());
    let prompt = HumanMessagePromptTemplate::new(template_jinja2!(
        "Dame un nombre creativo para una tienda que vende: {{producto}}",
        "producto"
    ));

    let get_name_chain = LLMChainBuilder::new()
        .prompt(prompt)
        .llm(llm.clone())
        .output_key("name")
        .build()
        .unwrap();

    let prompt = HumanMessagePromptTemplate::new(template_jinja2!(
        "Dame un slogan para el sigiente nombre: {{name}}",
        "name"
    ));
    let get_slogan_chain = LLMChainBuilder::new()
        .prompt(prompt)
        .llm(llm.clone())
        .output_key("slogan")
        .build()
        .unwrap();

    let sequential_chain = sequential_chain!(get_name_chain, get_slogan_chain);

    print!("Please enter a product: ");
    io::stdout().flush().unwrap(); // Display prompt to terminal

    let mut product = String::new();
    io::stdin().read_line(&mut product).unwrap(); // Get product from terminal input

    let product = product.trim();
    let output = sequential_chain
        .execute(prompt_args! {
            "producto" => product
        })
        .await
        .unwrap();

    println!("Name: {}", output["name"]);
    println!("Slogan: {}", output["slogan"]);
}
