use super::{
    ExtraConfig, OpenAIClient, PromptType, SendData,
    Model, TokensCountFactors,
};

use crate::{
    render::ReplyHandler,
    utils::PromptKind,
};

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::{Client as ReqwestClient, RequestBuilder};
use reqwest_eventsource::{Error as EventSourceError, Event, RequestBuilderExt};
use serde::Deserialize;
use serde_json::{json, Value};
use std::env;

const API_BASE: &str = "https://api.openai.com/v1";

const MODELS: [(&str, usize); 7] = [
    ("gpt-3.5-turbo", 4096),
    ("gpt-3.5-turbo-16k", 16385),
    ("gpt-3.5-turbo-1106", 16385),
    ("gpt-4-1106-preview", 128000),
    ("gpt-4-vision-preview", 128000),
    ("gpt-4", 8192),
    ("gpt-4-32k", 32768),
];

pub const OPENAI_TOKENS_COUNT_FACTORS: TokensCountFactors = (5, 2);

#[derive(Debug, Clone, Deserialize, Default)]
pub struct OpenAIConfig {
    pub name: Option<String>,
    pub api_key: Option<String>,
    pub organization_id: Option<String>,
    pub extra: Option<ExtraConfig>,
}

openai_compatible_client!(OpenAIClient);

impl OpenAIClient {
    config_get_fn!(api_key, get_api_key);

    pub const PROMPTS: [PromptType<'static>; 1] =
        [("api_key", "API Key:", true, PromptKind::String)];

    pub fn list_models(local_config: &OpenAIConfig) -> Vec<Model> {
        let client_name = Self::name(local_config);
        MODELS
            .into_iter()
            .map(|(name, max_tokens)| {
                Model::new(client_name, name)
                    .set_max_tokens(Some(max_tokens))
                    .set_tokens_count_factors(OPENAI_TOKENS_COUNT_FACTORS)
            })
            .collect()
    }

    fn request_builder(&self, client: &ReqwestClient, data: SendData) -> Result<RequestBuilder> {
        let api_key = self.get_api_key()?;

        let body = openai_build_body(data, self.model.name.clone());

        let env_prefix = Self::name(&self.config).to_uppercase();
        let api_base = env::var(format!("{env_prefix}_API_BASE"))
            .ok()
            .unwrap_or_else(|| API_BASE.to_string());

        let url = format!("{api_base}/chat/completions");

        debug!("OpenAI Request: {url} {body}");

        let mut builder = client.post(url).bearer_auth(api_key).json(&body);

        if let Some(organization_id) = &self.config.organization_id {
            builder = builder.header("OpenAI-Organization", organization_id);
        }

        Ok(builder)
    }
}

pub async fn openai_send_message(builder: RequestBuilder) -> Result<String> {
    let data: Value = builder.send().await?.json().await?;
    if let Some(err_msg) = data["error"]["message"].as_str() {
        bail!("{err_msg}");
    }

    let output = data["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| anyhow!("Invalid response data: {data}"))?;

    Ok(output.to_string())
}

pub async fn openai_send_message_streaming(
    builder: RequestBuilder,
    handler: &mut ReplyHandler,
) -> Result<()> {
    let mut es = builder.eventsource()?;
    while let Some(event) = es.next().await {
        match event {
            Ok(Event::Open) => {}
            Ok(Event::Message(message)) => {
                if message.data == "[DONE]" {
                    break;
                }
                let data: Value = serde_json::from_str(&message.data)?;
                if let Some(text) = data["choices"][0]["delta"]["content"].as_str() {
                    handler.text(text)?;
                }
            }
            Err(err) => {
                match err {
                    EventSourceError::InvalidStatusCode(_, res) => {
                        let data: Value = res.json().await?;
                        if let Some(err_msg) = data["error"]["message"].as_str() {
                            bail!("{err_msg}");
                        }
                        bail!("Request failed");
                    }
                    EventSourceError::StreamEnded => {}
                    _ => {
                        bail!("{}", err);
                    }
                }
                es.close();
            }
        }
    }

    Ok(())
}

pub fn openai_build_body(data: SendData, model: String) -> Value {
    let SendData {
        messages,
        temperature,
        stream,
    } = data;

    let mut body = json!({
        "model": model,
        "messages": messages,
    });

    // The default max_tokens of gpt-4-vision-preview is only 16, we need to make it larger
    if model == "gpt-4-vision-preview" {
        body["max_tokens"] = json!(4096);
    }

    if let Some(v) = temperature {
        body["temperature"] = v.into();
    }
    if stream {
        body["stream"] = true.into();
    }
    body
}
