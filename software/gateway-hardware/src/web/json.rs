//! JSON responses, rendered into a buffer before anything is sent.
//!
//! picoserve's own `Json` formats straight into the socket's transmit
//! buffer, and when the room left in it (a ring buffer, so not necessarily
//! the whole size) doesn't fit the response, it formats the response again
//! from the start for each further piece. That loses bytes for some
//! responses here (the connection is then reset after the headers); a
//! response that is in one piece sidesteps it.
//!
//! The buffer is one static one that the web tasks take turns with: a
//! buffer per response would live in the web tasks' futures, once per task.

use super::error::ApiError;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::{Mutex, MutexGuard};
use picoserve::response::{Body, Content, HeadersIter, Response, StatusCode};
use serde::Serialize;

/// The largest response body.
const SIZE: usize = 3072;

static BUFFER: Mutex<CriticalSectionRawMutex, [u8; SIZE]> = Mutex::new([0; SIZE]);

/// The right to render one response. Hold it only for as long as needed:
/// the other web tasks wait for it. Take it *before* locking anything else
/// that others wait for (the device table), so that those aren't held up
/// by the wait.
pub struct JsonBuffer(MutexGuard<'static, CriticalSectionRawMutex, [u8; SIZE]>);

/// Waits for the buffer.
pub async fn acquire() -> JsonBuffer {
    JsonBuffer(BUFFER.lock().await)
}

/// A rendered JSON document; holds the buffer until it is sent.
pub struct JsonBody {
    buffer: JsonBuffer,
    len: usize,
}

impl Content for JsonBody {
    fn content_type(&self) -> &'static str {
        "application/json"
    }

    fn content_length(&self) -> usize {
        self.len
    }

    async fn write_content<W: picoserve::io::Write>(self, mut writer: W) -> Result<(), W::Error> {
        writer.write_all(&self.buffer.0[..self.len]).await
    }
}

impl JsonBuffer {
    /// `value` rendered, as a body that doesn't borrow it (for a value that
    /// borrows locals of the handler).
    pub fn rendered(mut self, value: &impl Serialize) -> Result<JsonBody, ApiError> {
        let len = serde_json_core::to_slice(value, &mut self.0[..]).map_err(|_| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "response_too_large",
                "the response does not fit the buffer",
            )
        })?;
        Ok(JsonBody { buffer: self, len })
    }

    /// `value` as a JSON response with the given status.
    pub fn render_with_status(
        mut self,
        status: StatusCode,
        value: &impl Serialize,
    ) -> Result<Response<impl HeadersIter, impl Body>, ApiError> {
        let len = serde_json_core::to_slice(value, &mut self.0[..]).map_err(|_| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "response_too_large",
                "the response does not fit the buffer",
            )
        })?;
        Ok(Response::new(status, JsonBody { buffer: self, len }))
    }

    /// `value` as a `200 OK` JSON response.
    pub fn render(
        self,
        value: &impl Serialize,
    ) -> Result<Response<impl HeadersIter, impl Body>, ApiError> {
        self.render_with_status(StatusCode::OK, value)
    }
}

/// `value` as a `200 OK` JSON response.
pub async fn json(
    value: &impl Serialize,
) -> Result<Response<impl HeadersIter, impl Body>, ApiError> {
    acquire().await.render(value)
}

/// `value` as a JSON response with the given status.
pub async fn with_status(
    status: StatusCode,
    value: &impl Serialize,
) -> Result<Response<impl HeadersIter, impl Body>, ApiError> {
    acquire().await.render_with_status(status, value)
}
