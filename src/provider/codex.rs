use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::error::AppError;
use crate::provider::executor::Executor;
use crate::provider::Provider;

pub struct CodexProvider {
    executor: Arc<dyn Executor>,
}

impl CodexProvider {
    pub fn new(executor: Arc<dyn Executor>) -> Self {
        Self { executor }
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
        model: Option<&str>,
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

        let schema_path = schema_file.path().to_string_lossy().to_string();
        let mut args: Vec<String> = vec!["exec".into()];
        if let Some(m) = model {
            args.extend(["--model".into(), m.into()]);
        }
        args.extend([
            "--output-schema".into(),
            schema_path,
            "--skip-git-repo-check".into(),
        ]);

        let output = self
            .executor
            .run("codex", &args, prompt, timeout_secs)
            .await?;

        serde_json::from_str(&output.stdout).map_err(|e| AppError::OutputParse {
            message: format!("failed to parse output: {e}"),
            stdout: output.stdout,
        })
    }
}
