use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::error::AppError;
use crate::provider::executor::Executor;
use crate::provider::Provider;

pub struct GeminiProvider {
    executor: Arc<dyn Executor>,
}

impl GeminiProvider {
    pub fn new(executor: Arc<dyn Executor>) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl Provider for GeminiProvider {
    fn name(&self) -> &'static str {
        "gemini"
    }

    async fn execute(
        &self,
        prompt: &str,
        schema: &Value,
        model: Option<&str>,
        timeout_secs: Option<u64>,
    ) -> Result<Value, AppError> {
        let schema_str = serde_json::to_string_pretty(schema)
            .map_err(|e| AppError::InvalidSchema(format!("{e}")))?;

        let combined_prompt = format!(
            "{prompt}\n\n---\nRespond with JSON matching this schema:\n```json\n{schema_str}\n```"
        );

        let mut args: Vec<String> = Vec::new();
        if let Some(m) = model {
            args.extend(["--model".into(), m.into()]);
        }

        let output = self
            .executor
            .run("gemini", &args, &combined_prompt, timeout_secs)
            .await?;

        let json_str = extract_json(&output.stdout).unwrap_or(&output.stdout);

        serde_json::from_str(json_str).map_err(|e| AppError::OutputParse {
            message: format!("failed to parse output: {e}"),
            stdout: output.stdout,
        })
    }
}

fn extract_json(text: &str) -> Option<&str> {
    if let Some(start) = text.find("```json") {
        let content_start = start + 7;
        if let Some(end) = text[content_start..].find("```") {
            return Some(text[content_start..content_start + end].trim());
        }
    }
    if let Some(start) = text.find("```") {
        let content_start = start + 3;
        let content_start = text[content_start..]
            .find('\n')
            .map(|i| content_start + i + 1)
            .unwrap_or(content_start);
        if let Some(end) = text[content_start..].find("```") {
            return Some(text[content_start..content_start + end].trim());
        }
    }
    None
}
