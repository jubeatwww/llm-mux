use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{debug, error, warn};

use crate::error::AppError;

const DEFAULT_TIMEOUT_SECS: u64 = 120;

#[derive(Clone)]
pub struct CommandOutput {
    pub stdout: String,
    #[allow(dead_code)]
    pub stderr: String,
}

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait Executor: Send + Sync {
    async fn run(
        &self,
        program: &str,
        args: &[String],
        stdin_data: &str,
        timeout_secs: Option<u64>,
    ) -> Result<CommandOutput, AppError>;
}

pub struct CliExecutor;

impl CliExecutor {
    pub fn new() -> Arc<dyn Executor> {
        Arc::new(Self)
    }
}

impl Default for CliExecutor {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl Executor for CliExecutor {
    async fn run(
        &self,
        program: &str,
        args: &[String],
        stdin_data: &str,
        timeout_secs: Option<u64>,
    ) -> Result<CommandOutput, AppError> {
        let timeout_secs = timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS);

        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| AppError::ProviderExecution {
                message: format!("failed to spawn {program}: {e}"),
                stderr: String::new(),
            })?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(stdin_data.as_bytes()).await.map_err(|e| {
                AppError::ProviderExecution {
                    message: format!("failed to write to stdin: {e}"),
                    stderr: String::new(),
                }
            })?;
        }

        let output = timeout(Duration::from_secs(timeout_secs), child.wait_with_output())
            .await
            .map_err(|_| {
                warn!(
                    provider = program,
                    timeout_secs, "process timed out, killing"
                );
                AppError::Timeout {
                    provider: program.to_string(),
                    timeout_secs,
                }
            })?
            .map_err(|e| AppError::ProviderExecution {
                message: format!("failed to wait for {program}: {e}"),
                stderr: String::new(),
            })?;

        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();

        if !stderr.is_empty() {
            debug!(provider = program, stderr = %stderr, "stderr output");
        }

        if !output.status.success() {
            error!(provider = program, stderr = %stderr, "{program} failed");
            return Err(AppError::ProviderExecution {
                message: format!("{program} exited with status: {}", output.status),
                stderr,
            });
        }

        Ok(CommandOutput { stdout, stderr })
    }
}
