pub mod config;
pub mod device;
pub mod mapping;
pub mod ws;

use axum::{
    Json, Router,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use rust_i18n::t;
use serde::Serialize;
use serde_json::Value;
use std::{net::SocketAddrV4, thread};
use tokio::{fs, sync::{broadcast, mpsc::UnboundedSender, oneshot}};
use tower_http::services::ServeDir;

use crate::{
    mask::mask_command::MaskCommand,
    scrcpy::{control_msg::ScrcpyControlMsg, controller::ControllerCommand},
    utils::relate_to_root_path,
    web::ws::WebSocketNotification,
};

use once_cell::sync::Lazy;
use rand::Rng;

pub static API_KEY: Lazy<String> = Lazy::new(|| {
    let mut rng = rand::rng();
    (0..16)
        .map(|_| format!("{:02x}", rng.random::<u8>()))
        .collect()
});

async fn auth_middleware(req: axum::extract::Request, next: axum::middleware::Next) -> Result<Response, StatusCode> {
    let header_key = req.headers()
        .get("x-api-key")
        .and_then(|val| val.to_str().ok());

    let query_key = req.uri()
        .query()
        .and_then(|q| {
            q.split('&')
                .find(|pair| pair.starts_with("token="))
                .map(|pair| pair["token=".len()..].to_string())
        });

    let request_key = header_key.map(|s| s.to_string()).or(query_key);

    if let Some(key) = request_key {
        if key == *API_KEY {
            return Ok(next.run(req).await);
        }
    }

    log::warn!("[WebAuth] Unauthorized request to {}", req.uri().path());
    Err(StatusCode::UNAUTHORIZED)
}

pub struct Server;

impl Server {
    pub fn start(
        addr: SocketAddrV4,
        cs_tx: broadcast::Sender<ScrcpyControlMsg>,
        d_tx: UnboundedSender<ControllerCommand>,
        m_tx: crossbeam_channel::Sender<(MaskCommand, oneshot::Sender<Result<String, String>>)>,
        ws_tx: broadcast::Sender<WebSocketNotification>,
    ) {
        thread::spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async move {
                    Server::run_server(addr, cs_tx, d_tx, m_tx, ws_tx).await;
                });
        });
    }

    async fn run_server(
        addr: SocketAddrV4,
        cs_tx: broadcast::Sender<ScrcpyControlMsg>,
        d_tx: UnboundedSender<ControllerCommand>,
        m_tx: crossbeam_channel::Sender<(MaskCommand, oneshot::Sender<Result<String, String>>)>,
        ws_tx: broadcast::Sender<WebSocketNotification>,
    ) {
        log::info!("[WebServe] {}: {}", t!("web.server.startingOn"), addr);

        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

        let ip_str = if addr.ip().is_unspecified() || addr.ip().is_loopback() {
            "localhost"
        } else {
            &addr.ip().to_string()
        };
        let url = format!("http://{}:{}", ip_str, addr.port());
        log::info!(
            "[WebServe] {}: {}",
            t!("web.server.webServerAccessible"),
            url
        );

        opener::open(url).unwrap_or_else(|e| {
            log::error!("[WebServe] {}: {}", t!("web.server.failedToOpenBrowser"), e)
        });

        axum::serve(listener, Self::app(cs_tx, d_tx, m_tx, ws_tx))
            .await
            .unwrap();
    }

    fn app(
        cs_tx: broadcast::Sender<ScrcpyControlMsg>,
        d_tx: UnboundedSender<ControllerCommand>,
        m_tx: crossbeam_channel::Sender<(MaskCommand, oneshot::Sender<Result<String, String>>)>,
        ws_tx: broadcast::Sender<WebSocketNotification>,
    ) -> Router {
        let api_routes = Router::new()
            .nest(
                "/device",
                device::routers(cs_tx.clone(), d_tx.clone(), m_tx.clone()),
            )
            .nest("/mapping", mapping::routers(m_tx.clone()))
            .nest("/config", config::routers(m_tx.clone()))
            .nest("/ws", ws::routers(cs_tx, ws_tx, d_tx))
            .layer(axum::middleware::from_fn(auth_middleware));

        let router = Router::new()
            // Serve index.html with Cache-Control: no-store so the browser never
            // caches the entry point. JS/CSS assets have content hashes in their
            // filenames (Vite build) so they are naturally cache-busted.
            .route("/", get(serve_index))
            .route("/index.html", get(serve_index))
            .fallback_service(
                ServeDir::new(relate_to_root_path(["assets", "web"])).not_found_service(
                    axum::routing::any(serve_index)
                ),
            )
            .nest("/api", api_routes);

        #[cfg(debug_assertions)]
        {
            // allow CORS for development
            use tower_http::cors::{Any, CorsLayer};
            use axum::http::HeaderValue;

            let cors = CorsLayer::new()
                .allow_origin([
                    HeaderValue::from_static("http://localhost:5173"),
                    HeaderValue::from_static("http://127.0.0.1:5173"),
                ])
                .allow_methods(Any)
                .allow_headers(Any);

            return router.layer(cors);
        }
        #[cfg(not(debug_assertions))]
        return router;
    }
}

/// Serves index.html with `Cache-Control: no-store` so the browser always
/// fetches a fresh copy. Without this, browsers cache the HTML and keep
/// loading stale JS bundles even after a new build.
async fn serve_index() -> impl IntoResponse {
    let path = relate_to_root_path(["assets", "web", "index.html"]);
    match fs::read_to_string(&path).await {
        Ok(html) => {
            let injected_script = format!(
                "<script>window.API_KEY = \"{}\";</script></head>",
                *API_KEY
            );
            let modified_html = html.replace("</head>", &injected_script);
            (
                [
                    ("Content-Type", "text/html; charset=utf-8"),
                    ("Cache-Control", "no-store"),
                ],
                modified_html,
            )
                .into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

#[derive(Serialize)]
pub struct JsonResponse {
    pub code: u16,
    pub message: String,
    pub data: Option<Value>,
}

impl JsonResponse {
    pub fn new(code: u16, message: impl Into<String>, data: Option<Value>) -> Self {
        Self {
            code,
            message: message.into(),
            data,
        }
    }

    pub fn success(message: impl Into<String>, data: Option<Value>) -> Self {
        Self::new(200, message, data)
    }

    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::new(500, message, None)
    }
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(400, message, None)
    }
}

impl IntoResponse for JsonResponse {
    fn into_response(self) -> Response {
        (StatusCode::from_u16(self.code).unwrap(), Json(self)).into_response()
    }
}

#[derive(Debug)]
pub struct WebServerError(u16, String);

impl WebServerError {
    pub fn internal_error(message: impl Into<String>) -> Self {
        Self(500, message.into())
    }
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self(400, message.into())
    }
}

impl IntoResponse for WebServerError {
    fn into_response(self) -> Response {
        let res = JsonResponse {
            code: self.0,
            message: self.1,
            data: None,
        };
        log::error!(
            "[WebServe] {} ({}): {}",
            t!("web.server.responseError"),
            res.code,
            res.message
        );

        (StatusCode::from_u16(res.code).unwrap(), Json(res)).into_response()
    }
}
