use std::collections::HashMap;
use std::pin::Pin;

use crate::language_models::llm::LLM;
use crate::language_models::GenerateResult;
use crate::language_models::LLMError;
use crate::language_models::TokenUsage;
use crate::schemas::Message;
use crate::schemas::MessageType;
use crate::schemas::StreamData;
use async_trait::async_trait;
use futures::Stream;
use leap_connect::v1::api::Client as leap_client;
use leap_connect::v1::chat_completion;
use leap_connect::v1::chat_completion::ChatCompletionMessage;
use leap_connect::v1::chat_completion::ChatCompletionRequest;
use leap_connect::v1::common::MISTRAL;
use leap_connect::v1::error::APIError;
use tokio_stream::StreamExt;

#[derive(Clone)]
pub struct Tupleleap {
    pub(crate) client: leap_client,
    pub(crate) model: String,
}

const DEFAULT_MODEL: &str = MISTRAL;

impl Tupleleap {
    pub fn new(client: leap_client, model: String) -> Self {
        Tupleleap {
            client,
            model: model.into(),
        }
    }

    pub fn with_model(mut self, model: String) -> Self {
        self.model = model.into();
        self
    }

    fn generate_request(&self, messages: &[Message]) -> ChatCompletionRequest {
        let mapped_messages = messages
            .iter()
            .map(|message| message.clone().into())
            .collect();
        ChatCompletionRequest::new(self.model.clone(), mapped_messages)
    }
}

impl From<Message> for ChatCompletionMessage {
    fn from(message: Message) -> Self {
        ChatCompletionMessage {
            content: chat_completion::Content::Text(message.content),
            role: message.message_type.into(),
            name: message.id,
        }
    }
}

impl From<MessageType> for chat_completion::MessageRole {
    fn from(message_type: MessageType) -> Self {
        match message_type {
            MessageType::AIMessage => chat_completion::MessageRole::assistant,
            MessageType::ToolMessage => chat_completion::MessageRole::assistant,
            MessageType::SystemMessage => chat_completion::MessageRole::system,
            MessageType::HumanMessage => chat_completion::MessageRole::user,
        }
    }
}

#[async_trait]
impl LLM for Tupleleap {
    async fn generate(&self, messages: &[Message]) -> Result<GenerateResult, LLMError> {
        let request = self.generate_request(messages);
        let result = self.client.chat_completion(request).await?;
        if result.choices.is_empty() {
            return Err(APIError {
                message: "zero choices returned".into(),
            }
            .into());
        }
        let generation = match &result.choices[0].message.content {
            Some(msg) => msg,
            None => {
                return Err(APIError {
                    message: "No message in response".to_string(),
                }
                .into())
            }
        };

        Ok(GenerateResult {
            generation: generation.to_string(),
            tokens: Some(TokenUsage {
                prompt_tokens: result.usage.prompt_tokens as u32,
                completion_tokens: result.usage.completion_tokens as u32,
                total_tokens: result.usage.total_tokens as u32,
            }),
        })
    }

    async fn stream(
        &self,
        messages: &[Message],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamData, LLMError>> + Send>>, LLMError> {
        let request = self.generate_request(messages);
        let result_stream = self.client.chat_completion_stream(request).await?;

        let stream = result_stream.map(|data| {
            let choice = &data.choices[0];
            let content = match &choice.delta.content {
                Some(msg) => msg,
                None => {
                    return Err(LLMError::ContentNotFound(
                        "No message in response".to_string(),
                    ));
                }
            };
            Ok(StreamData::new(
                serde_json::to_value(HashMap::from([
                    ("created", data.created.to_string()),
                    ("model", data.model),
                ]))
                .unwrap_or_default(),
                content,
            ))
        });
        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    /*
    Add the following in settings.json file to run in vscode env
     "rust-analyzer.runnables.extraEnv": {
           "RUST_LOG": "debug",
           "TUPLELEAP_AI_API_KEY": "sk-xxxxxxx",
       },
       "rust-analyzer.cargo.extraEnv": {
           "RUST_LOG": "debug",
           "TUPLELEAP_AI_API_KEY": "sk-xxxxxxx",
       },
    */
    use super::*;
    use crate::llm::tupleleapai::client::Tupleleap;
    use leap_connect::v1::api::Client;
    use std::env;
    use tokio::io::AsyncWriteExt;
    use tokio_stream::StreamExt;

    #[tokio::test]
    async fn test_generate() {
        let client = Client::new(env::var("TUPLELEAP_AI_API_KEY").unwrap().to_string());
        let tupleleap = Tupleleap::new(client, "mistral".into());
        let response = tupleleap.invoke("Hey bro whatsup").await.unwrap();
        println!("{}", response);
    }

    #[tokio::test]
    async fn test_stream() {
        let client = Client::new(env::var("TUPLELEAP_AI_API_KEY").unwrap().to_string());
        let tupleleap = Tupleleap::new(client, "mistral".into());
        let message = Message::new_human_message("Why does water boil at 100 degrees?");
        let mut stream = tupleleap.stream(&vec![message]).await.unwrap();
        let mut stdout = tokio::io::stdout();
        while let Some(res) = stream.next().await {
            let data = res.unwrap();
            stdout.write_all(data.content.as_bytes()).await.unwrap();
        }
        stdout.write(b"\n").await.unwrap();
        stdout.flush().await.unwrap();
        let response = tupleleap.invoke("Hey bro whatsup").await.unwrap();
        println!("{}", response);
    }
}
