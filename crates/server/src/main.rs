mod ai;
mod auth;
mod config;
mod error;
mod handlers;

use std::net::SocketAddr;
use std::path::PathBuf;

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;
use skill_shelf_core::Shelf;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use config::ConfigStore;
use handlers::*;

/// Auth (signup / signin).
fn account_routes() -> Router<AppState> {
    Router::new()
        .route("/user/signup", post(signup))
        .route("/user/signin", post(signin))
}

/// Config: Skill Shelf's own settings + the config center (namespaces, service
/// tokens, and the consumer `resolve` endpoint).
fn config_routes() -> Router<AppState> {
    Router::new()
        // Skill Shelf's own settings (admin).
        .route("/config", get(get_config).put(put_config))
        // Config center — namespaces + versioning (admin).
        .route("/config/namespaces", get(list_namespaces))
        .route("/config/namespace", get(get_namespace).put(put_namespace))
        .route("/config/namespace/publish", post(publish_namespace))
        .route("/config/namespace/versions", get(list_versions))
        .route("/config/namespace/version", get(get_version))
        .route("/config/namespace/rollback", post(rollback_namespace))
        .route("/config/namespace/diff", get(diff_namespace))
        // Config center — clients / service tokens (admin).
        .route("/config/clients", get(list_clients).post(create_client))
        .route("/config/clients/{id}", axum::routing::delete(delete_client))
        // Config center — consumption (service token via X-Config-Token).
        .route("/config/resolve", get(resolve_config))
}

/// Skills, versions, and consumption (route / bundle / validate).
fn skill_routes() -> Router<AppState> {
    Router::new()
        .route("/skill", post(create_skill).get(list_skills))
        .route("/skill/by-name/{name}", get(get_skill_by_name))
        .route("/skill/{id}", get(get_skill).delete(delete_skill))
        .route("/skill/{id}/bundle", get(get_bundle))
        .route("/skill/{id}/branches", get(list_branches))
        .route("/skill/{id}/branch", post(create_branch))
        .route("/skill/{id}/commit", post(commit))
        .route("/skill/{id}/commits", get(list_commits))
        .route("/skill/{id}/rollback", post(rollback))
        .route("/skill/{id}/export", get(export_zip))
        .route("/skill/import", post(import_zip).layer(DefaultBodyLimit::max(50 * 1024 * 1024)))
        .route("/skill/import/github", post(import_github))
        .route("/commit/{cid}/tree", get(get_tree))
        .route("/commit/{cid}/file", get(get_file))
        .route("/diff", get(diff))
        .route("/route", post(route))
        .route("/validate", post(validate_files))
}

/// Feedback + AI refine (the optimization loop).
fn feedback_routes() -> Router<AppState> {
    Router::new()
        .route("/skill/{id}/feedback", post(add_feedback).get(list_feedback))
        .route("/skill/{id}/feedback/{fid}/status", post(set_feedback_status))
        .route("/skill/{id}/refine", post(refine))
        .route("/skill/{id}/refine/merge", post(refine_merge))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=info".into()))
        .init();

    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".into());
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8080);

    // Pick the backend by `DB`: a postgres URL → Postgres (CAS blobs still on
    // disk under DATA_DIR); otherwise SQLite at DATA_DIR.
    let db = std::env::var("DB").unwrap_or_default();
    let shelf = if db.starts_with("postgres://") || db.starts_with("postgresql://") {
        tracing::info!("using Postgres backend");
        Shelf::open_postgres(&db, &data_dir).expect("failed to open postgres shelf")
    } else {
        tracing::info!("using SQLite backend");
        Shelf::open(&data_dir).expect("failed to open shelf")
    };
    let config = ConfigStore::new(PathBuf::from(&data_dir));
    let state = AppState::new(shelf, config);
    state.bootstrap_admin();

    let app = Router::new()
        .route("/status", get(status))
        .merge(account_routes())
        .merge(config_routes())
        .merge(skill_routes())
        .merge(feedback_routes())
        .layer(axum::middleware::from_fn_with_state(state.clone(), auth::require_auth))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await.expect("failed to bind");
    tracing::info!("skill-shelf-server listening on http://{addr}  (data: {data_dir})");
    axum::serve(listener, app).await.expect("server error");
}
