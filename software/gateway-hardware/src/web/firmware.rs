//! Firmware updates: `PUT` the image as the raw request body.
//!
//! - `/firmware/gateway`: the gateway's own firmware; it restarts into the
//!   new image once that verified.
//! - `/devices/:device/firmware` and `/device-types/:type/firmware`: a CAN
//!   node (or all nodes of a type), over the CAN-OTA protocol.
//!
//! The size (`Content-Length`) and the CRC-32 (`?crc32=`, decimal) of the
//! image have to be known up front: the nodes and the OTA partition check
//! the image against them. Only one update runs at a time, `GET
//! /firmware/job` shows how far it is.

use super::devices::DeviceName;
use super::error::ApiError;
use super::json::json;
use crate::device_list::DEVICE_LIST;
use core::sync::atomic::{AtomicBool, Ordering};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::Duration;
use picoserve::extract::Query;
use picoserve::io::Read;
use picoserve::request::Request;
use picoserve::response::{IntoResponse, ResponseWriter, StatusCode};
use picoserve::routing::{get, parse_path_segment, put_service, PathRouter, RequestHandlerService};
use picoserve::{ResponseSent, Router};
use serde::{Deserialize, Serialize};

/// The OTA slot of the gateway (see `partitions.csv`).
const GATEWAY_MAX_SIZE: usize = 0x1C0000;
/// What a CAN node can be sent: an STM32/ESP32 node image.
const NODE_MAX_SIZE: usize = 0x100000;
/// How much of the image is handled in one go.
const CHUNK: usize = 512;
/// Receiving a whole image, which is paced by the CAN bus for nodes.
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(900);

pub fn routes<R: PathRouter>(router: Router<R>) -> Router<impl PathRouter> {
    router
        .route("/firmware/job", get(job))
        .route("/firmware/gateway", put_service(GatewayUpload))
        .route(
            ("/devices", parse_path_segment::<DeviceName>(), "/firmware"),
            put_service(DeviceUpload),
        )
        .route(
            ("/device-types", parse_path_segment::<u8>(), "/firmware"),
            put_service(TypeUpload),
        )
}

// ---- the running (or last) update ---------------------------------------

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum State {
    Idle,
    Running,
    Done,
    Failed,
}

#[derive(Clone, Copy, Serialize)]
struct Job {
    state: State,
    /// `gateway`, `device` or `type`.
    target: &'static str,
    total: usize,
    sent: usize,
    error: Option<&'static str>,
}

static JOB: Mutex<CriticalSectionRawMutex, Job> = Mutex::new(Job {
    state: State::Idle,
    target: "",
    total: 0,
    sent: 0,
    error: None,
});

/// Set while an update runs.
static BUSY: AtomicBool = AtomicBool::new(false);

async fn job() -> Result<impl IntoResponse, ApiError> {
    let job = *JOB.lock().await;
    json(&job).await
}

// ---- the upload ----------------------------------------------------------

#[derive(Clone, Copy)]
enum Target {
    Gateway,
    Device(u32),
    Type(u8),
}

impl Target {
    const fn name(self) -> &'static str {
        match self {
            Target::Gateway => "gateway",
            Target::Device(_) => "device",
            Target::Type(_) => "type",
        }
    }

    const fn max_size(self) -> usize {
        match self {
            Target::Gateway => GATEWAY_MAX_SIZE,
            _ => NODE_MAX_SIZE,
        }
    }
}

#[derive(Deserialize)]
struct CrcQuery {
    crc32: u32,
}

#[derive(Serialize)]
struct Uploaded {
    bytes: usize,
    /// The gateway restarts into the new firmware.
    restarting: bool,
}

/// Releases [`BUSY`] when the update ends, however it ends.
struct BusyGuard;

impl Drop for BusyGuard {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::Release);
    }
}

async fn fail(message: &'static str, status: StatusCode, code: &'static str) -> ApiError {
    let mut job = JOB.lock().await;
    job.state = State::Failed;
    job.error = Some(message);
    ApiError::new(status, code, message)
}

/// Receives the image from `reader` and hands it to `target`.
async fn receive<B: Read>(
    target: Target,
    size: usize,
    crc: u32,
    reader: &mut B,
) -> Result<Uploaded, ApiError> {
    if size == 0 || size > target.max_size() {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "size",
            "the image is empty or too large for the target",
        ));
    }
    if BUSY
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "busy",
            "another update is running",
        ));
    }
    let _busy = BusyGuard;
    *JOB.lock().await = Job {
        state: State::Running,
        target: target.name(),
        total: size,
        sent: 0,
        error: None,
    };

    let started = match target {
        Target::Gateway => crate::update::start(size as u32, crc).await,
        Target::Device(key) => {
            crate::can_update::by_uid_start(key, size as u32, crc).await;
            true
        }
        Target::Type(device_type) => {
            crate::can_update::by_type_start(device_type, size as u32, crc).await;
            true
        }
    };
    if !started {
        return Err(fail(
            "could not open the update partition",
            StatusCode::INTERNAL_SERVER_ERROR,
            "update_start",
        )
        .await);
    }

    let mut chunk = [0u8; CHUNK];
    let mut received = 0;
    while received < size {
        let want = chunk.len().min(size - received);
        let read = match reader.read(&mut chunk[..want]).await {
            Ok(0) | Err(_) => {
                abort(target).await;
                return Err(fail(
                    "the upload was interrupted",
                    StatusCode::BAD_REQUEST,
                    "interrupted",
                )
                .await);
            }
            Ok(read) => read,
        };
        let written = match target {
            Target::Gateway => crate::update::write(&chunk[..read]).await,
            _ => {
                crate::can_update::data(&chunk[..read]).await;
                true
            }
        };
        if !written {
            abort(target).await;
            return Err(fail(
                "writing the image failed",
                StatusCode::INTERNAL_SERVER_ERROR,
                "update_write",
            )
            .await);
        }
        received += read;
        JOB.lock().await.sent = received;
    }

    match target {
        Target::Gateway => {
            if !crate::update::finish().await {
                return Err(fail(
                    "the image did not verify (size or CRC-32 wrong)",
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "verify",
                )
                .await);
            }
            super::system::request_restart();
        }
        _ => crate::can_update::complete().await,
    }
    JOB.lock().await.state = State::Done;
    Ok(Uploaded {
        bytes: received,
        restarting: matches!(target, Target::Gateway),
    })
}

async fn abort(target: Target) {
    match target {
        Target::Gateway => crate::update::abort().await,
        _ => crate::can_update::abort().await,
    }
}

/// Reads the request's size and CRC, receives the image and writes the
/// answer. Common to the three routes.
async fn handle<R: Read, W: ResponseWriter<Error = R::Error>>(
    target: Result<Target, ApiError>,
    mut request: Request<'_, R>,
    response_writer: W,
) -> Result<ResponseSent, W::Error> {
    let state = &();
    let Query(CrcQuery { crc32: crc }) =
        picoserve::from_request_parts!(state, request, response_writer, Query<CrcQuery>);
    let size = request.body_connection.content_length();

    let result = match target {
        Err(error) => Err(error),
        Ok(target) => {
            let mut reader = request
                .body_connection
                .body()
                .reader()
                .with_different_timeout(UPLOAD_TIMEOUT);
            receive(target, size, crc, &mut reader).await
        }
    };

    let connection = request.body_connection.finalize().await?;
    match result {
        Ok(uploaded) => match json(&uploaded).await {
            Ok(response) => response_writer.write_response(connection, response).await,
            Err(error) => error.write_to(connection, response_writer).await,
        },
        Err(error) => error.write_to(connection, response_writer).await,
    }
}

struct GatewayUpload;

impl RequestHandlerService<(), ()> for GatewayUpload {
    async fn call_request_handler_service<R: Read, W: ResponseWriter<Error = R::Error>>(
        &self,
        _state: &(),
        _path_parameters: (),
        request: Request<'_, R>,
        response_writer: W,
    ) -> Result<ResponseSent, W::Error> {
        handle(Ok(Target::Gateway), request, response_writer).await
    }
}

struct DeviceUpload;

impl RequestHandlerService<(), (DeviceName,)> for DeviceUpload {
    async fn call_request_handler_service<R: Read, W: ResponseWriter<Error = R::Error>>(
        &self,
        _state: &(),
        (name,): (DeviceName,),
        request: Request<'_, R>,
        response_writer: W,
    ) -> Result<ResponseSent, W::Error> {
        let target = DEVICE_LIST
            .lock()
            .await
            .resolve(name.as_str())
            .map(Target::Device)
            .ok_or(ApiError::not_found("no such device"));
        handle(target, request, response_writer).await
    }
}

struct TypeUpload;

impl RequestHandlerService<(), (u8,)> for TypeUpload {
    async fn call_request_handler_service<R: Read, W: ResponseWriter<Error = R::Error>>(
        &self,
        _state: &(),
        (device_type,): (u8,),
        request: Request<'_, R>,
        response_writer: W,
    ) -> Result<ResponseSent, W::Error> {
        handle(Ok(Target::Type(device_type)), request, response_writer).await
    }
}
