use leap_chain::embedding::{
    embedder_trait::Embedder, ollama::ollama_embedder::OllamaEmbedder,
};

#[tokio::main]
async fn main() {
    let ollama = OllamaEmbedder::default().with_model("nomic-embed-text");

    let response = ollama.embed_query("Why is the sky blue?").await.unwrap();

    println!("{:?}", response);
}
