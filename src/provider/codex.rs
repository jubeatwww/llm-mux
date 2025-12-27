use async_trait::async_trait;
use serde_json::Value;

use crate::error::AppError;
use crate::provider::executor::{run_cli, run_cli_with_timeout};
use crate::provider::Provider;

pub struct CodexProvider;

impl CodexProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CodexProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for CodexProvider {
    fn name(&self) -> &'static str {
        "codex"
    }

    async fn execute(
        &self,
        prompt: &str,
        schema: &Value,
        model: &str,
        timeout_secs: Option<u64>,
    ) -> Result<Value, AppError> {
        let schema_file = tempfile::Builder::new()
            .suffix(".json")
            .tempfile()
            .map_err(|e| AppError::ProviderExecution {
                message: format!("failed to create temp file: {e}"),
                stderr: String::new(),
            })?;

        std::fs::write(schema_file.path(), serde_json::to_string(schema).unwrap()).map_err(
            |e| AppError::ProviderExecution {
                message: format!("failed to write schema: {e}"),
                stderr: String::new(),
            },
        )?;

        let schema_path = schema_file.path().to_string_lossy();
        let args = [
            "exec",
            "--model",
            model,
            "--output-schema",
            &schema_path,
            "--skip-git-repo-check",
        ];

        let output = match timeout_secs {
            Some(t) => run_cli_with_timeout("codex", &args, prompt, t).await?,
            None => run_cli("codex", &args, prompt).await?,
        };

        serde_json::from_str(&output.stdout).map_err(|e| AppError::OutputParse {
            message: format!("failed to parse output: {e}"),
            stdout: output.stdout,
        })
    }
}