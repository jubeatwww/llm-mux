mod config;
mod error;
mod provider;
mod rate_limiter;
mod schema;

use std::collections::HashMap;
use std::sync::Arc;

use actix_web::{web, App, HttpResponse, HttpServer};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use crate::config::{Config, ModelSettings, ProviderSettings};
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
    provider_settings: HashMap<String, ProviderSettings>,
}

async fn health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({"status": "ok"}))
}

async fn generate(
    state: web::Data<Arc<AppState>>,
    req: web::Json<GenerateRequest>,
) -> Result<HttpResponse, AppError> {
    schema::validate_structured_schema(&req.schema)?;

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
            let provider_cfg = state.provider_settings.get(&req.provider);

            let supports_auto = provider_cfg.map(|p| p.supports_auto_model).unwrap_or(true);

            if !supports_auto {
                return Err(AppError::AutoModelNotSupported(req.provider.clone()));
            }

            // Use provider-level rate limit for auto model
            let guard = if provider_cfg.is_some() {
                state
                    .rate_limiter
                    .try_acquire(&req.provider, "_auto")
                    .map_err(|()| AppError::RateLimited {
                        provider: req.provider.clone(),
                        model: None,
                    })
                    .ok()
            } else {
                None
            };

            let timeout = provider_cfg.and_then(|p| p.timeout_secs);
            (timeout, guard)
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

    schema::validate_output(&req.schema, &output)?;

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
    let provider_settings = config.provider_settings();

    let rate_limiter = RateLimiter::new();
    for (key, settings) in &model_settings {
        info!(provider = %key.0, model = %key.1, "registering model settings");
        rate_limiter.register(key.0.clone(), key.1.clone(), settings.clone());
    }

    // Register provider-level rate limits for auto model
    for (name, settings) in &provider_settings {
        if settings.supports_auto_model {
            let auto_settings = ModelSettings {
                rps: settings.rps,
                rpm: settings.rpm,
                concurrent: settings.concurrent,
                timeout_secs: settings.timeout_secs,
            };
            info!(provider = %name, "registering auto model settings");
            rate_limiter.register(name.clone(), "_auto".into(), auto_settings);
        }
    }

    let state = Arc::new(AppState {
        executor: CliExecutor::new(),
        rate_limiter,
        model_settings,
        provider_settings,
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

    fn valid_schema() -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "message": { "type": "string" }
            }
        })
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
        rate_limiter.register("claude".into(), "_auto".into(), settings.clone());
        rate_limiter.register("gemini".into(), "_auto".into(), settings.clone());

        let mut model_settings = HashMap::new();
        model_settings.insert(("claude".into(), "sonnet".into()), settings);

        let mut provider_settings = HashMap::new();
        provider_settings.insert(
            "claude".into(),
            ProviderSettings {
                supports_auto_model: true,
                rps: Some(10),
                rpm: Some(100),
                concurrent: Some(2),
                timeout_secs: Some(60),
            },
        );
        provider_settings.insert(
            "gemini".into(),
            ProviderSettings {
                supports_auto_model: true,
                rps: Some(10),
                rpm: Some(100),
                concurrent: Some(2),
                timeout_secs: Some(60),
            },
        );
        provider_settings.insert(
            "codex".into(),
            ProviderSettings {
                supports_auto_model: false,
                ..Default::default()
            },
        );

        Arc::new(AppState {
            executor,
            rate_limiter,
            model_settings,
            provider_settings,
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
            "schema": valid_schema()
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
            "schema": valid_schema()
        }))
        .await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn test_auto_model_not_supported() {
        let resp = post_generate(serde_json::json!({
            "provider": "codex",
            "prompt": "hello",
            "schema": valid_schema()
        }))
        .await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn test_auto_model_supported() {
        let resp = post_generate(serde_json::json!({
            "provider": "claude",
            "prompt": "hello",
            "schema": valid_schema()
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
            "schema": valid_schema()
        }))
        .await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn test_missing_required_field() {
        let resp = post_generate(serde_json::json!({
            "provider": "claude",
            "model": "sonnet",
            "schema": valid_schema()
        }))
        .await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn test_invalid_schema_missing_type() {
        let resp = post_generate(serde_json::json!({
            "provider": "claude",
            "prompt": "hello",
            "schema": {
                "properties": {
                    "message": { "type": "string" }
                }
            }
        }))
        .await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn test_invalid_schema_wrong_type() {
        let resp = post_generate(serde_json::json!({
            "provider": "claude",
            "prompt": "hello",
            "schema": {
                "type": "array",
                "items": { "type": "string" }
            }
        }))
        .await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn test_invalid_schema_missing_properties() {
        let resp = post_generate(serde_json::json!({
            "provider": "claude",
            "prompt": "hello",
            "schema": {
                "type": "object"
            }
        }))
        .await;
        assert_eq!(resp.status(), 400);
    }
}
