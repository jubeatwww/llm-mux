mod claude;
mod codex;
mod executor;
mod gemini;

use async_trait::async_trait;
use serde_json::Value;

use crate::error::AppError;

pub use claude::ClaudeProvider;
pub use codex::CodexProvider;
pub use gemini::GeminiProvider;

#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &'static str;

    async fn execute(
        &self,
        prompt: &str,
        schema: &Value,
        model: &str,
        timeout_secs: Option<u64>,
    ) -> Result<Value, AppError>;
}

pub fn get_provider(name: &str) -> Option<Box<dyn Provider>> {
    match name {
        "codex" => Some(Box::new(CodexProvider::new())),
        "claude" => Some(Box::new(ClaudeProvider::new())),
        "gemini" => Some(Box::new(GeminiProvider::new())),
        _ => None,
    }
}
