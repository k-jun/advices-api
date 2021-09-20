use axum::{
    extract::{Extension, Path},
    handler::{delete, get},
    http::StatusCode,
    response::IntoResponse,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    convert::Infallible,
    net::SocketAddr,
    sync::{Arc, RwLock},
    time::Duration,
};
use tower::{BoxError, ServiceBuilder};
use tower_http::{add_extension::AddExtensionLayer, trace::TraceLayer};

#[tokio::main]
async fn main() {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "tower_http=debug")
    }
    tracing_subscriber::fmt::init();

    let db = Db::default();

    // Compose the routes
    let app = Router::new()
        .route("/", get(root))
        .route("/advices", get(advices_index).post(advices_create))
        .route("/advices/:id", delete(advices_delete))
        // Add middleware to all routes
        .layer(
            ServiceBuilder::new()
                .timeout(Duration::from_secs(10))
                .layer(TraceLayer::new_for_http())
                .layer(AddExtensionLayer::new(db))
                .into_inner(),
        )
        .handle_error(|error: BoxError| {
            let result = if error.is::<tower::timeout::error::Elapsed>() {
                Ok(StatusCode::REQUEST_TIMEOUT)
            } else {
                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Unhandled internal error: {}", error),
                ))
            };

            Ok::<_, Infallible>(result)
        })
        // Make sure all errors have been handled
        .check_infallible();

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn root() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

async fn advices_index(Extension(db): Extension<Db>) -> impl IntoResponse {
    let advices = db.read().unwrap();
    let advices = advices.values().cloned().collect::<Vec<_>>();

    Json(advices)
}

async fn advices_create(Extension(db): Extension<Db>) -> impl IntoResponse {
    let advice = advices_generate().await.unwrap();
    db.write().unwrap().insert(advice.id, advice.clone());

    (StatusCode::CREATED, Json(advice))
}

async fn advices_delete(Path(id): Path<i64>, Extension(db): Extension<Db>) -> impl IntoResponse {
    if db.write().unwrap().remove(&id).is_some() {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn advices_generate() -> Result<Advice, anyhow::Error> {
    let resp = reqwest::get("https://api.adviceslip.com/advice")
        .await?
        .json::<HashMap<String, Advice>>()
        .await?;

    let advice = resp.get("slip").unwrap();
    Ok(advice.clone())
}

type Db = Arc<RwLock<HashMap<i64, Advice>>>;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Advice {
    id: i64,
    advice: String,
}
