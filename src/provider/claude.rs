use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::error::AppError;
use crate::provider::executor::Executor;
use crate::provider::Provider;

pub struct ClaudeProvider {
    executor: Arc<dyn Executor>,
}

impl ClaudeProvider {
    pub fn new(executor: Arc<dyn Executor>) -> Self {
        Self { executor }
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
        model: Option<&str>,
        timeout_secs: Option<u64>,
    ) -> Result<Value, AppError> {
        let schema_compact =
            serde_json::to_string(schema).map_err(|e| AppError::InvalidSchema(format!("{e}")))?;

        let mut args: Vec<String> = Vec::new();
        if let Some(m) = model {
            args.extend(["--model".into(), m.into()]);
        }
        args.extend([
            "--output-format".into(),
            "json".into(),
            "--json-schema".into(),
            schema_compact,
            "-p".into(),
        ]);

        let output = self
            .executor
            .run("claude", &args, prompt, timeout_secs)
            .await?;

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
