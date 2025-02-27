#[macro_use]
mod common;
mod message;
mod model;

pub use common::*;
pub use message::*;
pub use model::*;

register_client!(
    (openai, "openai", OpenAIConfig, OpenAIClient),
    (localai, "localai", LocalAIConfig, LocalAIClient),
    (
        azure_openai,
        "azure-openai",
        AzureOpenAIConfig,
        AzureOpenAIClient
    ),
    (palm, "palm", PaLMConfig, PaLMClient),
    (ernie, "ernie", ErnieConfig, ErnieClient),
    (qianwen, "qianwen", QianwenConfig, QianwenClient),
);
