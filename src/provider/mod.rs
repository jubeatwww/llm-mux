mod claude;
mod codex;
pub mod executor;
mod gemini;

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::error::AppError;

pub use claude::ClaudeProvider;
pub use codex::CodexProvider;
pub use executor::{CliExecutor, Executor};
pub use gemini::GeminiProvider;

#[async_trait]
pub trait Provider: Send + Sync {
    #[allow(dead_code)]
    fn name(&self) -> &'static str;

    async fn execute(
        &self,
        prompt: &str,
        schema: &Value,
        model: Option<&str>,
        timeout_secs: Option<u64>,
    ) -> Result<Value, AppError>;
}

pub fn get_provider_with_executor(
    name: &str,
    executor: Arc<dyn Executor>,
) -> Option<Box<dyn Provider>> {
    match name {
        "codex" => Some(Box::new(CodexProvider::new(executor))),
        "claude" => Some(Box::new(ClaudeProvider::new(executor))),
        "gemini" => Some(Box::new(GeminiProvider::new(executor))),
        _ => None,
    }
}
