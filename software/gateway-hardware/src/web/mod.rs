//! The web server: a REST API under `/api/v1` and the browser UI that uses
//! it. See `gateway-core/docs/api-rest.md` for the routes.
//!
//! Every route sits behind [`BasicAuth`] (ADR 0008: no bypass, not even for
//! read-only ones). One task serves one connection at a time, so several of
//! them are started; each builds the (zero-sized) router itself.
//!
//! HTTPS (ADR 0018): the UI and the API are served over TLS on port 443
//! with the certificate of [`crate::cert`]; port 80 only answers
//! `308 Permanent Redirect` to the same URL on `https://` (308 keeps the
//! method and body, so a script's `PUT` still arrives as a `PUT`). One TLS
//! session runs at a time ([`crate::https::slot`]). `web-https off` on the
//! console serves everything over plain HTTP on port 80 instead, the way
//! back in if TLS ever doesn't work.

mod assets;
mod commands;
mod devices;
mod error;
mod firmware;
mod json;
mod mqtt;
mod sensors;
mod settings;
mod system;
mod tls;

use crate::config;
use embassy_executor::Spawner;
use embassy_net::Stack;
use embassy_time::Duration;
use gateway_core::web::{credentials_match, decode_basic_auth};
use picoserve::io::Read;
use picoserve::response::{IntoResponse, StatusCode};
use picoserve::routing::{get, post, Layer, Next, PathRouter};
use picoserve::{Config, ResponseSent, Router, Timeouts};

/// Plain HTTP (`web-https off`): connections served at the same time.
const HTTP_TASKS: usize = 3;
/// HTTPS: connections that can be open at the same time. They are served
/// one after the other (one TLS session at a time), but each needs its own
/// listening socket, or a browser's parallel connections are refused; so
/// these tasks are cheap and share the session's buffers.
const HTTPS_TASKS: usize = 5;
const HTTP_PORT: u16 = 80;
const HTTPS_PORT: u16 = 443;
/// How long a TLS handshake may take (the ECDH and the signature are done
/// in software on a 160 MHz core).
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);
/// How long an accepted connection may stay silent before it is dropped
/// (browsers open connections in advance and may never use them).
const FIRST_BYTE_TIMEOUT: Duration = Duration::from_secs(10);

static SERVER_CONFIG: Config = Config::new(Timeouts {
    start_read_request: Duration::from_secs(5),
    persistent_start_read_request: Duration::from_secs(1),
    read_request: Duration::from_secs(5),
    write: Duration::from_secs(5),
})
.keep_connection_alive();

/// Starts the web server tasks on `stack`. `https`: see the module docs.
pub fn spawn(spawner: &Spawner, stack: Stack<'static>, https: bool) {
    spawner.spawn(system::restart_task().unwrap());
    if https {
        for _ in 0..HTTPS_TASKS {
            spawner.spawn(https_task(stack).unwrap());
        }
        spawner.spawn(redirect_task(stack).unwrap());
    } else {
        for id in 0..HTTP_TASKS {
            spawner.spawn(http_task(id, stack).unwrap());
        }
    }
}

// The servers' futures, with the buffers they hold, live on the heap (the
// otherwise unused RAM above the statics): there they don't take away the
// stack's room the way the tasks' static storage would.

#[embassy_executor::task(pool_size = HTTP_TASKS)]
async fn http_task(id: usize, stack: Stack<'static>) -> ! {
    alloc::boxed::Box::pin(serve_plain(id, stack)).await
}

#[embassy_executor::task(pool_size = HTTPS_TASKS)]
async fn https_task(stack: Stack<'static>) -> ! {
    alloc::boxed::Box::pin(serve_tls(stack)).await
}

#[embassy_executor::task]
async fn redirect_task(stack: Stack<'static>) -> ! {
    alloc::boxed::Box::pin(serve_redirect(HTTPS_TASKS, stack)).await
}

async fn serve_plain(id: usize, stack: Stack<'static>) -> ! {
    let app = app();
    let mut http_buffer = [0u8; 1536];
    let mut tcp_rx = [0u8; 4096];
    let mut tcp_tx = [0u8; 1024];
    picoserve::Server::new(&app, &SERVER_CONFIG, &mut http_buffer)
        .listen_and_serve(id, stack, HTTP_PORT, &mut tcp_rx, &mut tcp_tx)
        .await
        .into_never()
}

/// Port 80 with HTTPS on: every request is sent to `https://`.
async fn serve_redirect(id: usize, stack: Stack<'static>) -> ! {
    let app = Router::new().layer(RedirectToHttps);
    let mut http_buffer = [0u8; 1024];
    let mut tcp_rx = [0u8; 1024];
    let mut tcp_tx = [0u8; 512];
    picoserve::Server::new(&app, &SERVER_CONFIG, &mut http_buffer)
        .listen_and_serve(id, stack, HTTP_PORT, &mut tcp_rx, &mut tcp_tx)
        .await
        .into_never()
}

/// Port 443: accepts a connection, waits until the client sends something,
/// then for the TLS slot, and serves the connection ([`serve_session`]).
///
/// This future only holds the socket's buffers (~3 KB): several of these
/// wait at the same time. The session's own future (the handshake and
/// picoserve, ~20 KB) is put on the heap only while it runs, and only one
/// runs at a time.
async fn serve_tls(stack: Stack<'static>) -> ! {
    use embassy_time::with_timeout;

    let mut tcp_rx = [0u8; 2048];
    let mut tcp_tx = [0u8; 1024];
    loop {
        let mut socket = embassy_net::tcp::TcpSocket::new(stack, &mut tcp_rx, &mut tcp_tx);
        if socket.accept(HTTPS_PORT).await.is_err() {
            continue;
        }
        socket.set_keep_alive(Some(Duration::from_secs(30)));
        socket.set_timeout(Some(Duration::from_secs(45)));

        // Only a connection that brings a ClientHello gets to wait for the
        // slot.
        let first = with_timeout(FIRST_BYTE_TIMEOUT, socket.read_with(|data| (0, data.len())));
        if matches!(first.await, Ok(Ok(1..))) {
            let mut slot = crate::https::slot().await;
            alloc::boxed::Box::pin(serve_session(&mut socket, &mut slot)).await;
        }
        socket.close();
        let _ = with_timeout(Duration::from_secs(1), socket.flush()).await;
        socket.abort();
    }
}

/// One TLS connection: handshake, then requests until it ends.
async fn serve_session(
    socket: &mut embassy_net::tcp::TcpSocket<'_>,
    buffers: &mut crate::https::Buffers,
) {
    use crate::https::{self, TcpStream, TlsConnection};
    use embassy_time::with_timeout;
    use mbedtls_rs::{ServerSessionConfig, Session, SessionConfig};

    let (Some(tls), Some(credentials)) = (https::tls(), crate::cert::server_credentials().await)
    else {
        return;
    };
    let session_config = SessionConfig::Server(ServerSessionConfig::new(credentials));
    let Ok(mut session) = Session::new(tls, TcpStream(socket), &session_config) else {
        return;
    };
    if let Ok(Ok(())) = with_timeout(HANDSHAKE_TIMEOUT, session.connect()).await {
        if let Ok((read, write)) = session.split().await {
            let app = app();
            let _ = picoserve::Server::new(&app, &SERVER_CONFIG, &mut buffers.http)
                .serve(TlsConnection::new(read, write, &mut buffers.out))
                .await;
        }
        let _ = with_timeout(Duration::from_secs(1), session.close()).await;
    }
}

fn app() -> Router<impl PathRouter> {
    let api = Router::new()
        .route("/system", get(system::get_system))
        .route("/system/restart", post(system::restart));
    let api = devices::routes(api);
    let api = commands::routes(api);
    let api = settings::routes(api);
    let api = mqtt::routes(api);
    let api = firmware::routes(api);
    let api = sensors::routes(api);
    let api = tls::routes(api);
    let app = Router::new().nest("/api/v1", api);
    assets::routes(app).layer(BasicAuth)
}

/// Lets only requests through that carry the configured web credentials in
/// an `Authorization: Basic` header.
struct BasicAuth;

impl<State, PathParameters> Layer<State, PathParameters> for BasicAuth {
    type NextState = State;
    type NextPathParameters = PathParameters;

    async fn call_layer<
        'a,
        R: Read + 'a,
        NextLayer: Next<'a, R, State, PathParameters>,
        W: picoserve::response::ResponseWriter<Error = R::Error>,
    >(
        &self,
        next: NextLayer,
        state: &State,
        path_parameters: PathParameters,
        request_parts: picoserve::request::RequestParts<'_>,
        response_writer: W,
    ) -> Result<ResponseSent, W::Error> {
        let given = request_parts
            .headers()
            .get("Authorization")
            .and_then(|value| value.as_str().ok())
            .and_then(decode_basic_auth);
        let authorized = match given {
            Some(given) => {
                let (username, password) = config::web_credentials().await;
                credentials_match(&given, username.as_str(), password.as_str())
            }
            None => false,
        };

        if authorized {
            next.run(state, path_parameters, response_writer).await
        } else {
            (
                StatusCode::UNAUTHORIZED,
                [(
                    "WWW-Authenticate",
                    "Basic realm=\"gateway\", charset=\"UTF-8\"",
                )],
                "Unauthorized",
            )
                .write_to(next.into_connection().await?, response_writer)
                .await
        }
    }
}

/// Answers every request with `308 Permanent Redirect` to the same path on
/// `https://`, at the host the client asked for (or the configured FQDN).
struct RedirectToHttps;

impl<State, PathParameters> Layer<State, PathParameters> for RedirectToHttps {
    type NextState = State;
    type NextPathParameters = PathParameters;

    async fn call_layer<
        'a,
        R: Read + 'a,
        NextLayer: Next<'a, R, State, PathParameters>,
        W: picoserve::response::ResponseWriter<Error = R::Error>,
    >(
        &self,
        next: NextLayer,
        _state: &State,
        _path_parameters: PathParameters,
        request_parts: picoserve::request::RequestParts<'_>,
        response_writer: W,
    ) -> Result<ResponseSent, W::Error> {
        let host = request_parts
            .headers()
            .get("Host")
            .and_then(|value| value.as_str().ok())
            .and_then(gateway_core::web::host_without_port)
            .map(heapless::String::<{ crate::cert::FQDN_CAP }>::try_from)
            .and_then(Result::ok);
        let host = match host {
            Some(host) => host,
            None => crate::cert::effective_fqdn().await,
        };
        let mut location = heapless::String::<512>::new();
        let written = gateway_core::web::https_location(
            &mut location,
            &host,
            request_parts.path().encoded(),
            request_parts.query().map(|q| q.0),
        );
        let connection = next.into_connection().await?;
        if written.is_err() {
            return StatusCode::URI_TOO_LONG
                .write_to(connection, response_writer)
                .await;
        }
        (
            StatusCode::PERMANENT_REDIRECT,
            [("Location", location.as_str())],
            "",
        )
            .write_to(connection, response_writer)
            .await
    }
}
