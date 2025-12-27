mod config;
mod error;
mod provider;
mod rate_limiter;

use std::collections::HashMap;
use std::sync::Arc;

use actix_web::{web, App, HttpResponse, HttpServer};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use crate::config::{Config, ModelSettings};
use crate::error::AppError;
use crate::provider::{get_provider_with_executor, CliExecutor, Executor};
use crate::rate_limiter::RateLimiter;

#[derive(Debug, Deserialize)]
struct GenerateRequest {
    provider: String,
    model: Option<String>,
    prompt: String,
    schema: Value,
}

#[derive(Debug, Serialize)]
struct GenerateResponse {
    output: Value,
}

struct AppState {
    executor: Arc<dyn Executor>,
    rate_limiter: RateLimiter,
    model_settings: HashMap<(String, String), ModelSettings>,
    supports_auto_model: HashMap<String, bool>,
}

async fn health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({"status": "ok"}))
}

async fn generate(
    state: web::Data<Arc<AppState>>,
    req: web::Json<GenerateRequest>,
) -> Result<HttpResponse, AppError> {
    let provider = get_provider_with_executor(&req.provider, state.executor.clone())
        .ok_or_else(|| AppError::ProviderNotFound(req.provider.clone()))?;

    let (timeout_secs, _guard) = match &req.model {
        Some(model) => {
            let key = (req.provider.clone(), model.clone());
            if !state.model_settings.contains_key(&key) {
                return Err(AppError::ModelNotFound {
                    provider: req.provider.clone(),
                    model: req.model.clone(),
                });
            }

            let guard = state
                .rate_limiter
                .try_acquire(&req.provider, model)
                .map_err(|()| AppError::RateLimited {
                    provider: req.provider.clone(),
                    model: req.model.clone(),
                })?;

            let timeout = state.model_settings.get(&key).and_then(|s| s.timeout_secs);
            (timeout, Some(guard))
        }
        None => {
            let supports_auto = state
                .supports_auto_model
                .get(&req.provider)
                .copied()
                .unwrap_or(true);

            if !supports_auto {
                return Err(AppError::AutoModelNotSupported(req.provider.clone()));
            }

            (None, None)
        }
    };

    info!(
        provider = %req.provider,
        model = ?req.model,
        timeout_secs = ?timeout_secs,
        "executing request"
    );

    let output = provider
        .execute(&req.prompt, &req.schema, req.model.as_deref(), timeout_secs)
        .await?;

    Ok(HttpResponse::Ok().json(GenerateResponse { output }))
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive("llm_mux=info".parse().unwrap()),
        )
        .init();

    let config_path = std::env::var("LLM_MUX_CONFIG").unwrap_or_else(|_| "config.toml".to_string());

    let config = match Config::load(&config_path) {
        Ok(c) => c,
        Err(e) => {
            warn!(
                "failed to load config from {}: {}, using defaults",
                config_path, e
            );
            Config {
                server: config::ServerConfig::default(),
                providers: vec![],
            }
        }
    };

    let model_settings = config.model_settings();
    let supports_auto_model = config.provider_supports_auto_model();

    let rate_limiter = RateLimiter::new();
    for (key, settings) in &model_settings {
        info!(provider = %key.0, model = %key.1, "registering model settings");
        rate_limiter.register(key.0.clone(), key.1.clone(), settings.clone());
    }

    let state = Arc::new(AppState {
        executor: CliExecutor::new(),
        rate_limiter,
        model_settings,
        supports_auto_model,
    });

    let bind_addr = format!("{}:{}", config.server.host, config.server.port);
    info!("starting server on {}", bind_addr);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .route("/health", web::get().to(health))
            .route("/generate", web::post().to(generate))
    })
    .bind(&bind_addr)?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::executor::{CommandOutput, MockExecutor};
    use actix_web::{dev::ServiceResponse, test};

    fn mock_executor() -> Arc<dyn Executor> {
        let mut mock = MockExecutor::new();
        mock.expect_run().returning(|_, _, _, _| {
            Ok(CommandOutput {
                stdout: r#"{"structured_output": {"message": "hello"}}"#.to_string(),
                stderr: String::new(),
            })
        });
        Arc::new(mock)
    }

    fn test_state(executor: Arc<dyn Executor>) -> Arc<AppState> {
        let settings = ModelSettings {
            rps: Some(10),
            rpm: Some(100),
            concurrent: Some(2),
            timeout_secs: Some(60),
        };

        let rate_limiter = RateLimiter::new();
        rate_limiter.register("claude".into(), "sonnet".into(), settings.clone());

        let mut model_settings = HashMap::new();
        model_settings.insert(("claude".into(), "sonnet".into()), settings);

        let mut supports_auto_model = HashMap::new();
        supports_auto_model.insert("claude".into(), true);
        supports_auto_model.insert("gemini".into(), true);
        supports_auto_model.insert("codex".into(), false);

        Arc::new(AppState {
            executor,
            rate_limiter,
            model_settings,
            supports_auto_model,
        })
    }

    async fn post_generate(body: Value) -> ServiceResponse {
        let state = test_state(mock_executor());
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .route("/generate", web::post().to(generate)),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/generate")
            .set_json(body)
            .to_request();

        test::call_service(&app, req).await
    }

    #[actix_web::test]
    async fn test_provider_not_found() {
        let resp = post_generate(serde_json::json!({
            "provider": "unknown",
            "model": "test",
            "prompt": "hello",
            "schema": {}
        }))
        .await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn test_model_not_found() {
        let resp = post_generate(serde_json::json!({
            "provider": "claude",
            "model": "unknown-model",
            "prompt": "hello",
            "schema": {}
        }))
        .await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn test_auto_model_not_supported() {
        let resp = post_generate(serde_json::json!({
            "provider": "codex",
            "prompt": "hello",
            "schema": {}
        }))
        .await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn test_auto_model_supported() {
        let resp = post_generate(serde_json::json!({
            "provider": "claude",
            "prompt": "hello",
            "schema": {}
        }))
        .await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn test_valid_request_with_model() {
        let resp = post_generate(serde_json::json!({
            "provider": "claude",
            "model": "sonnet",
            "prompt": "hello",
            "schema": {}
        }))
        .await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn test_missing_required_field() {
        let resp = post_generate(serde_json::json!({
            "provider": "claude",
            "model": "sonnet",
            "schema": {}
        }))
        .await;
        assert_eq!(resp.status(), 400);
    }
}
