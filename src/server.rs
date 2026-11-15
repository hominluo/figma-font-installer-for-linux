//! HTTP surface: the `/figma/*` API plus the CORS/PNA gate browsers need to
//! reach a loopback service from `https://www.figma.com`.

use std::{path::Path, sync::Arc, time::Instant};

use axum::{
    body::Body,
    extract::{Query, Request, State},
    http::{
        header::{self, HeaderName, HeaderValue},
        HeaderMap, Method, StatusCode,
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use tokio::{net::TcpListener, sync::RwLock};

use crate::{
    config::Settings,
    fonts::{self, FontIndex},
    preview, proto,
};

/// Figma's web app is the only origin the official agent allows.
const ALLOWED_ORIGINS: [&str; 3] = [
    "https://www.figma.com",
    "https://figma.com",
    "https://www.figma.com:443",
];

#[derive(Clone)]
pub struct AppState {
    pub index: Arc<RwLock<FontIndex>>,
    pub settings: Arc<Settings>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/figma/version", get(version))
        .route("/figma/font-files", get(font_files))
        .route("/figma/font-file", get(font_file))
        .route("/figma/font-preview", get(font_preview))
        .route("/figma/health", get(health))
        .route("/figma/clear-data", post(clear_data))
        .route("/figma/desktop/can-open-url", get(can_open_url))
        .with_state(state.clone())
        .layer(middleware::from_fn_with_state(state, gate))
}

/// Binds every configured port, keeping whatever succeeds. Port 44960 is only a
/// fallback for Figma builds that probe it, so a failure there is not fatal.
pub async fn bind(ports: &[u16]) -> Vec<TcpListener> {
    let mut listeners = Vec::new();

    for port in ports {
        match TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, *port)).await {
            Ok(listener) => {
                info!("listening on http://127.0.0.1:{port}");
                listeners.push(listener);
            }
            Err(error) => {
                warn!("could not bind 127.0.0.1:{port}: {error}");
            }
        }
    }

    listeners
}

pub async fn serve(state: AppState, listeners: Vec<TcpListener>) -> anyhow::Result<()> {
    let app = router(state);
    let mut tasks = Vec::new();

    for listener in listeners {
        let app = app.clone();
        tasks.push(tokio::spawn(async move {
            if let Err(error) = axum::serve(listener, app).await {
                error!("server stopped: {error}");
            }
        }));
    }

    for task in tasks {
        let _ = task.await;
    }

    Ok(())
}

async fn version(State(state): State<AppState>) -> Json<proto::VersionResponse> {
    Json(proto::version_response(&state.settings))
}

async fn font_files(State(state): State<AppState>) -> Json<proto::FontFilesResponse> {
    let index = state.index.read().await;
    info!(
        "/figma/font-files -> {} faces in {} files",
        index.face_count,
        index.files.len()
    );
    Json(proto::font_files_response(&index, &state.settings))
}

async fn health() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], "ok")
}

#[derive(Debug, Deserialize)]
struct FileQuery {
    file: Option<String>,
}

async fn font_file(State(state): State<AppState>, Query(query): Query<FileQuery>) -> Response {
    let Some(key) = query.file else {
        return bad_request("missing `file` query parameter");
    };

    let path = {
        let index = state.index.read().await;
        match index.files.get(&key) {
            // Only files from the scan index are served: `?file=/etc/shadow` is
            // not a valid path even though it would pass a naive `exists()`.
            Some(file) => file.path.clone(),
            None => return not_found("unknown font file"),
        }
    };

    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            let mime = font_mime(&path);
            let mut response = Response::new(Body::from(bytes));
            response
                .headers_mut()
                .insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
            response
        }
        Err(error) => {
            error!("could not read {}: {error}", path.display());
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not read font file",
            )
                .into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
struct PreviewQuery {
    file: Option<String>,
    family: Option<String>,
    style: Option<String>,
    postscript: Option<String>,
    font_size: Option<f32>,
}

async fn font_preview(
    State(state): State<AppState>,
    Query(query): Query<PreviewQuery>,
) -> Response {
    if !state.settings.enable_preview {
        return not_found("font previews are disabled");
    }

    let Some(key) = query.file.clone() else {
        return bad_request("missing `file` query parameter");
    };

    let family = query.family.unwrap_or_default();
    let style = query.style.unwrap_or_default();
    let postscript = query.postscript.unwrap_or_default();

    // Resolve the face while holding the index lock, then drop it so the file
    // read and outline extraction do not block rescans.
    let selection = {
        let index = state.index.read().await;
        let Some(file) = index.files.get(&key) else {
            return not_found("unknown font file");
        };
        let Some((face, instance)) = file.find(&family, &style, &postscript) else {
            return not_found("no matching face");
        };

        Selection {
            path: file.path.clone(),
            face_index: face.index,
            text: if family.is_empty() {
                face.family.clone()
            } else {
                family
            },
            axes: face.axes.clone(),
            coords: instance.map(|i| i.coords.clone()).unwrap_or_default(),
        }
    };

    let data = match tokio::fs::read(&selection.path).await {
        Ok(data) => data,
        Err(error) => {
            error!("could not read {}: {error}", selection.path.display());
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not read font file",
            )
                .into_response();
        }
    };

    // Figma sends point sizes; SVG user units are CSS pixels.
    let size_px = query.font_size.unwrap_or(16.0) / 72.0 * 96.0;

    let rendered = tokio::task::spawn_blocking(move || {
        preview::render_svg(
            &data,
            selection.face_index,
            &selection.text,
            size_px,
            &selection.axes,
            &selection.coords,
        )
    })
    .await;

    match rendered {
        Ok(Some(svg)) => {
            let mut response = Response::new(Body::from(svg));
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("image/svg+xml"),
            );
            response
        }
        Ok(None) => not_found("could not render a preview"),
        Err(error) => {
            error!("preview task failed: {error}");
            (StatusCode::INTERNAL_SERVER_ERROR, "preview failed").into_response()
        }
    }
}

struct Selection {
    path: std::path::PathBuf,
    face_index: u32,
    text: String,
    axes: Vec<crate::fonts::Axis>,
    coords: Vec<f32>,
}

/// Figma occasionally asks the agent to drop its cache (after a font install).
/// Rebuilding also happens on the background timer, so this is a convenience.
async fn clear_data(State(state): State<AppState>) -> StatusCode {
    rescan(&state).await;
    StatusCode::OK
}

async fn can_open_url() -> impl IntoResponse {
    // We only serve fonts; "open in desktop app" stays a browser concern.
    Json(false)
}

async fn rescan(state: &AppState) {
    let settings = state.settings.clone();
    let previous = state.index.read().await.clone();

    match tokio::task::spawn_blocking(move || fonts::scan(&settings, Some(&previous))).await {
        Ok(Ok(index)) => {
            *state.index.write().await = index;
        }
        Ok(Err(error)) => warn!("font rescan failed: {error:#}"),
        Err(error) => warn!("font rescan task failed: {error}"),
    }
}

pub fn spawn_rescan_timer(state: AppState) {
    let interval = state.settings.rescan_interval;
    if interval.is_zero() {
        return;
    }

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(interval).await;
            rescan(&state).await;
        }
    });
}

// ---------------------------------------------------------------------------
// Middleware
// ---------------------------------------------------------------------------

async fn gate(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let started = Instant::now();
    let method = request.method().clone();
    let path = request.uri().path().to_string();

    // DNS-rebinding protection: a hostile page can resolve its own domain to
    // 127.0.0.1, but it cannot forge the `Host` header.
    if !host_allowed(
        request
            .headers()
            .get(header::HOST)
            .and_then(|value| value.to_str().ok()),
        &state.settings.ports,
    ) {
        debug!("{method} {path} -> 403 (bad Host header)");
        return (StatusCode::FORBIDDEN, "invalid Host header").into_response();
    }

    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let requested_headers = request
        .headers()
        .get(header::ACCESS_CONTROL_REQUEST_HEADERS)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);

    if method == Method::OPTIONS {
        let mut response = StatusCode::NO_CONTENT.into_response();
        apply_cors(
            response.headers_mut(),
            origin.as_deref(),
            requested_headers.as_deref(),
        );
        debug!("{method} {path} -> 204 (preflight)");
        return response;
    }

    let mut response = next.run(request).await;
    apply_cors(
        response.headers_mut(),
        origin.as_deref(),
        requested_headers.as_deref(),
    );

    debug!(
        "{method} {path} -> {} in {} ms",
        response.status().as_u16(),
        started.elapsed().as_millis()
    );

    response
}

fn apply_cors(headers: &mut HeaderMap, origin: Option<&str>, requested_headers: Option<&str>) {
    match origin {
        Some(origin) if ALLOWED_ORIGINS.contains(&origin) => {
            if let Ok(value) = HeaderValue::from_str(origin) {
                headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
            }
        }
        // Non-browser clients (curl, the test suite) do not send Origin.
        None => {
            headers.insert(
                header::ACCESS_CONTROL_ALLOW_ORIGIN,
                HeaderValue::from_static(ALLOWED_ORIGINS[0]),
            );
        }
        Some(_) => {}
    }

    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        match requested_headers {
            Some(value) => HeaderValue::from_str(value)
                .unwrap_or_else(|_| HeaderValue::from_static("Content-Type, Accept")),
            None => HeaderValue::from_static("Content-Type, Accept"),
        },
    );
    headers.insert(
        header::ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_static("600"),
    );
    headers.insert(
        HeaderName::from_static("access-control-allow-private-network"),
        HeaderValue::from_static("true"),
    );
    headers.append(header::VARY, HeaderValue::from_static("Origin"));
}

fn host_allowed(host: Option<&str>, ports: &[u16]) -> bool {
    let Some(host) = host else {
        return false;
    };

    let (name, port) = split_host(host);

    if !matches!(name, "127.0.0.1" | "localhost" | "[::1]") {
        return false;
    }

    match port {
        Some(port) => ports.is_empty() || ports.contains(&port),
        None => true,
    }
}

fn split_host(host: &str) -> (&str, Option<u16>) {
    if host.starts_with('[') {
        if let Some(end) = host.find(']') {
            let name = &host[..=end];
            let port = host[end + 1..]
                .strip_prefix(':')
                .and_then(|p| p.parse().ok());
            return (name, port);
        }
    }

    match host.rsplit_once(':') {
        Some((name, port)) if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => {
            (name, port.parse().ok())
        }
        _ => (host, None),
    }
}

fn font_mime(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "ttc" | "otc" => "font/collection",
        _ => "application/octet-stream",
    }
}

fn not_found(message: &'static str) -> Response {
    (StatusCode::NOT_FOUND, message).into_response()
}

fn bad_request(message: &'static str) -> Response {
    (StatusCode::BAD_REQUEST, message).into_response()
}

#[cfg(test)]
mod tests {
    use super::{host_allowed, split_host};

    #[test]
    fn host_header_validation() {
        let ports = [44950, 44960];

        assert_eq!(split_host("127.0.0.1:44950"), ("127.0.0.1", Some(44950)));
        assert_eq!(split_host("localhost"), ("localhost", None));
        assert_eq!(split_host("[::1]:44950"), ("[::1]", Some(44950)));

        assert!(host_allowed(Some("127.0.0.1:44950"), &ports));
        assert!(host_allowed(Some("localhost:44960"), &ports));
        assert!(host_allowed(Some("localhost"), &ports));
        assert!(!host_allowed(Some("127.0.0.1:8080"), &ports));
        assert!(!host_allowed(Some("evil.example:44950"), &ports));
        assert!(!host_allowed(None, &ports));
    }
}
