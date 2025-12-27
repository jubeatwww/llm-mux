use async_trait::async_trait;
use serde_json::Value;

use crate::error::AppError;
use crate::provider::executor::{run_cli, run_cli_with_timeout};
use crate::provider::Provider;

pub struct ClaudeProvider;

impl ClaudeProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ClaudeProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for ClaudeProvider {
    fn name(&self) -> &'static str {
        "claude"
    }

    async fn execute(
        &self,
        prompt: &str,
        schema: &Value,
        model: &str,
        timeout_secs: Option<u64>,
    ) -> Result<Value, AppError> {
        let schema_compact =
            serde_json::to_string(schema).map_err(|e| AppError::InvalidSchema(format!("{e}")))?;

        let args = [
            "--model",
            model,
            "--output-format",
            "json",
            "--json-schema",
            &schema_compact,
            "-p",
        ];

        let output = match timeout_secs {
            Some(t) => run_cli_with_timeout("claude", &args, prompt, t).await?,
            None => run_cli("claude", &args, prompt).await?,
        };

        let response: Value =
            serde_json::from_str(&output.stdout).map_err(|e| AppError::OutputParse {
                message: format!("failed to parse output: {e}"),
                stdout: output.stdout.clone(),
            })?;

        response
            .get("structured_output")
            .cloned()
            .ok_or_else(|| AppError::OutputParse {
                message: "missing 'structured_output' field".to_string(),
                stdout: output.stdout,
            })
    }
}