//! The web server: a REST API under `/api/v1` and the browser UI that uses
//! it. See `gateway-core/docs/api-rest.md` for the routes.
//!
//! Every route sits behind [`BasicAuth`] (ADR 0008: no bypass, not even for
//! read-only ones). One [`web_task`] serves one connection at a time, so
//! [`TASKS`] of them are started; each builds the (zero-sized) router itself.
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

/// How many connections can be open at the same time: with HTTPS, all but
/// the last task take TLS connections (served one after the other) and the
/// last one answers the redirects on port 80.
pub const TASKS: usize = 3;
const HTTP_PORT: u16 = 80;
const HTTPS_PORT: u16 = 443;
/// How long a TLS handshake may take (the ECDH and the signature are done
/// in software on a 160 MHz core).
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);

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
    for id in 0..TASKS {
        spawner.spawn(web_task(id, stack, https).unwrap());
    }
}

#[embassy_executor::task(pool_size = TASKS)]
async fn web_task(id: usize, stack: Stack<'static>, https: bool) -> ! {
    // The server's future, with the buffers it holds, is ~12 KB. On the heap
    // (the otherwise unused RAM above the statics) it doesn't take away the
    // stack's room the way the task's static storage would.
    if !https {
        alloc::boxed::Box::pin(serve_plain(id, stack)).await
    } else if id + 1 < TASKS {
        alloc::boxed::Box::pin(serve_tls(stack)).await
    } else {
        alloc::boxed::Box::pin(serve_redirect(id, stack)).await
    }
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

/// Port 443: accepts a connection, waits for the TLS slot, does the
/// handshake and serves requests until the connection ends.
async fn serve_tls(stack: Stack<'static>) -> ! {
    use crate::https::{self, TcpStream, TlsConnection};
    use embassy_time::with_timeout;
    use mbedtls_rs::{ServerSessionConfig, Session, SessionConfig};

    let app = app();
    let mut http_buffer = [0u8; 1536];
    let mut tcp_rx = [0u8; 4096];
    let mut tcp_tx = [0u8; 1024];
    loop {
        let mut socket = embassy_net::tcp::TcpSocket::new(stack, &mut tcp_rx, &mut tcp_tx);
        if socket.accept(HTTPS_PORT).await.is_err() {
            continue;
        }
        socket.set_keep_alive(Some(Duration::from_secs(30)));
        socket.set_timeout(Some(Duration::from_secs(45)));

        let slot = https::slot().await;
        let session_config = match (https::tls(), crate::cert::server_credentials().await) {
            (Some(tls), Some(credentials)) => Some((
                tls,
                SessionConfig::Server(ServerSessionConfig::new(credentials)),
            )),
            _ => None,
        };
        if let Some((tls, session_config)) = session_config {
            if let Ok(mut session) = Session::new(tls, TcpStream(&mut socket), &session_config) {
                if let Ok(Ok(())) = with_timeout(HANDSHAKE_TIMEOUT, session.connect()).await {
                    if let Ok((read, write)) = session.split().await {
                        let _ = picoserve::Server::new(&app, &SERVER_CONFIG, &mut http_buffer)
                            .serve(TlsConnection { read, write })
                            .await;
                    }
                    let _ = with_timeout(Duration::from_secs(1), session.close()).await;
                }
            }
        }
        drop(slot);
        socket.close();
        let _ = with_timeout(Duration::from_secs(1), socket.flush()).await;
        socket.abort();
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
