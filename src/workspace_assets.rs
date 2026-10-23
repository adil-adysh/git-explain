//! Self-contained browser assets. Rebuild frontend/dist when frontend sources
//! change; the distribution binary has no Node or filesystem asset dependency.
use axum::{
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Response},
};
use sha2::{Digest, Sha256};

const SCRIPT: &[u8] = include_bytes!("../frontend/dist/workspace.js");
const STYLE: &[u8] = include_bytes!("../frontend/dist/workspace.css");

pub async fn shell() -> Response {
    let html = include_str!("../frontend/dist/index.html")
        .replace(
            "/ui-assets/workspace.js",
            &format!("/ui-assets/workspace.js?v={:x}", Sha256::digest(SCRIPT)),
        )
        .replace(
            "/ui-assets/workspace.css",
            &format!("/ui-assets/workspace.css?v={:x}", Sha256::digest(STYLE)),
        );
    let mut response = Html(html).into_response();
    response.headers_mut().insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; base-uri 'none'; frame-ancestors 'none'; form-action 'self'"));
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

fn asset(body: &'static [u8], content_type: &'static str, request: HeaderMap) -> Response {
    let etag = format!("\"{:x}\"", Sha256::digest(body));
    let mut response = if request
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        == Some(&etag)
    {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        ([(header::CONTENT_TYPE, content_type)], body).into_response()
    };
    response.headers_mut().insert(
        header::ETAG,
        HeaderValue::from_str(&etag).expect("SHA256 is a header value"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}
pub async fn script(headers: HeaderMap) -> Response {
    asset(SCRIPT, "text/javascript; charset=utf-8", headers)
}
pub async fn style(headers: HeaderMap) -> Response {
    asset(STYLE, "text/css; charset=utf-8", headers)
}
