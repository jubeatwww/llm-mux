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
use crate::provider::get_provider;
use crate::rate_limiter::RateLimiter;

#[derive(Debug, Deserialize)]
struct GenerateRequest {
    provider: String,
    model: String,
    prompt: String,
    schema: Value,
}

#[derive(Debug, Serialize)]
struct GenerateResponse {
    output: Value,
}

struct AppState {
    rate_limiter: RateLimiter,
    model_settings: HashMap<(String, String), ModelSettings>,
}

async fn health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({"status": "ok"}))
}

async fn generate(
    state: web::Data<Arc<AppState>>,
    req: web::Json<GenerateRequest>,
) -> Result<HttpResponse, AppError> {
    let provider = get_provider(&req.provider)
        .ok_or_else(|| AppError::ProviderNotFound(req.provider.clone()))?;

    let _guard = state
        .rate_limiter
        .try_acquire(&req.provider, &req.model)
        .map_err(|()| AppError::RateLimited {
            provider: req.provider.clone(),
            model: req.model.clone(),
        })?;

    let timeout_secs = state
        .model_settings
        .get(&(req.provider.clone(), req.model.clone()))
        .and_then(|s| s.timeout_secs);

    info!(
        provider = %req.provider,
        model = %req.model,
        timeout_secs = ?timeout_secs,
        "executing request"
    );

    let output = provider
        .execute(&req.prompt, &req.schema, &req.model, timeout_secs)
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

    let rate_limiter = RateLimiter::new();
    for (key, settings) in &model_settings {
        info!(provider = %key.0, model = %key.1, "registering model settings");
        rate_limiter.register(key.0.clone(), key.1.clone(), settings.clone());
    }

    let state = Arc::new(AppState {
        rate_limiter,
        model_settings,
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