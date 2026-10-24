//! The error body every failing API route answers with:
//! `{"error":"<code>","message":"<text>"}` and the matching status.

use picoserve::response::{IntoResponse, Json, StatusCode};
use serde::Serialize;

#[derive(Debug, Clone, Copy)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: &'static str,
}

#[derive(Serialize)]
struct Body {
    error: &'static str,
    message: &'static str,
}

impl ApiError {
    pub const fn new(status: StatusCode, code: &'static str, message: &'static str) -> Self {
        Self {
            status,
            code,
            message,
        }
    }

    pub const fn not_found(message: &'static str) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }

    pub const fn invalid(message: &'static str) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid", message)
    }
}

impl IntoResponse for ApiError {
    async fn write_to<
        R: picoserve::io::Read,
        W: picoserve::response::ResponseWriter<Error = R::Error>,
    >(
        self,
        connection: picoserve::response::Connection<'_, R>,
        response_writer: W,
    ) -> Result<picoserve::ResponseSent, W::Error> {
        let response = Json(Body {
            error: self.code,
            message: self.message,
        })
        .into_response()
        .with_status_code(self.status);
        response_writer.write_response(connection, response).await
    }
}
