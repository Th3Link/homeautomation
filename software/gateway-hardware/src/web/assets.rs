//! The browser UI: the three files built by `gateway-web` (`npm run build`
//! there), gzip'd and baked into the firmware.
//!
//! The UI routes by path (`/`, `/sensors`, `/mqtt`, `/gateway`, `/https`,
//! `/devices/:device`), so each of those answers with the same `index.html`;
//! the code then shows the page for the path it was loaded from.

use super::devices::DeviceName;
use picoserve::response::{IntoResponse, Response, StatusCode};
use picoserve::routing::{get, parse_path_segment, PathRouter};
use picoserve::Router;

const INDEX_HTML: &[u8] = include_bytes!("../../../gateway-web/dist/index.html.gz");
const APP_JS: &[u8] = include_bytes!("../../../gateway-web/dist/app.js.gz");
const STYLE_CSS: &[u8] = include_bytes!("../../../gateway-web/dist/style.css.gz");

pub fn routes<R: PathRouter>(router: Router<R>) -> Router<impl PathRouter> {
    router
        .route("/", get(index))
        .route("/mqtt", get(index))
        .route("/sensors", get(index))
        .route("/gateway", get(index))
        .route("/https", get(index))
        .route(
            ("/devices", parse_path_segment::<DeviceName>()),
            get(device_index),
        )
        .route("/assets/app.js", get(app_js))
        .route("/assets/style.css", get(style_css))
}

fn gzipped(bytes: &'static [u8], content_type: &'static str) -> impl IntoResponse {
    Response::new(StatusCode::OK, bytes)
        .with_content_type(content_type)
        .with_header("Content-Encoding", "gzip")
        // The files change with the firmware; they are small enough to ask
        // for again.
        .with_header("Cache-Control", "no-cache")
}

async fn index() -> impl IntoResponse {
    gzipped(INDEX_HTML, "text/html; charset=utf-8")
}

async fn device_index(_device: DeviceName) -> impl IntoResponse {
    gzipped(INDEX_HTML, "text/html; charset=utf-8")
}

async fn app_js() -> impl IntoResponse {
    gzipped(APP_JS, "text/javascript; charset=utf-8")
}

async fn style_css() -> impl IntoResponse {
    gzipped(STYLE_CSS, "text/css; charset=utf-8")
}
