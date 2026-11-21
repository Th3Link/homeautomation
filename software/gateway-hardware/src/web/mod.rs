//! The web server: a REST API under `/api/v1` and the browser UI that uses
//! it. See `gateway-core/docs/api-rest.md` for the routes.
//!
//! Every route sits behind `BasicAuth` (ADR 0008: no bypass, not even for
//! read-only ones). One task serves one connection at a time, so several of
//! them are started; each builds the (zero-sized) router itself.
//!
//! The gateway serves the UI and API on port 443. Connections share one
//! active TLS session through [`crate::https::slot`].

mod assets;
mod automation;
mod commands;
mod devices;
mod error;
mod firmware;
mod json;
mod location;
mod logs;
mod mqtt;
mod network;
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

/// HTTPS: connections that can be open at the same time. They are served
/// one after the other (one TLS session at a time), but each needs its own
/// listening socket, or a browser's parallel connections are refused; so
/// these tasks are cheap and share the session's buffers.
const HTTPS_TASKS: usize = 5;
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

/// Starts the HTTPS listeners on port 443.
pub fn spawn(spawner: &Spawner, stack: Stack<'static>) {
    spawner.spawn(system::restart_task().unwrap());
    for buffers in listener_buffers() {
        spawner.spawn(https_task(stack, buffers).unwrap());
    }
}

struct TcpBuffers {
    rx: [u8; 1536],
    tx: [u8; 1024],
}

/// Each ROM boot stack has room for one listener next to an MQTT connection.
/// The remaining three share SRAM1 with the allocators.
fn listener_buffers() -> [&'static mut TcpBuffers; HTTPS_TASKS] {
    use core::mem::MaybeUninit;
    use core::sync::atomic::{AtomicBool, Ordering};
    static TAKEN: AtomicBool = AtomicBool::new(false);
    #[unsafe(link_section = ".rom_boot_pro_uninit")]
    static mut PRO: MaybeUninit<TcpBuffers> = MaybeUninit::uninit();
    #[unsafe(link_section = ".rom_boot_app_uninit")]
    static mut APP: MaybeUninit<TcpBuffers> = MaybeUninit::uninit();
    #[unsafe(link_section = ".dram2_uninit")]
    static mut REST: MaybeUninit<[TcpBuffers; HTTPS_TASKS - 2]> = MaybeUninit::uninit();
    assert!(!TAKEN.swap(true, Ordering::Relaxed));
    // SAFETY: this is called once after the ROM stack is retired. Each listener
    // receives a distinct pair of byte arrays, zeroed before any reference exists.
    unsafe {
        let pro = (&raw mut PRO).cast::<TcpBuffers>();
        let app = (&raw mut APP).cast::<TcpBuffers>();
        let rest = (&raw mut REST).cast::<[TcpBuffers; HTTPS_TASKS - 2]>();
        pro.write_bytes(0, 1);
        app.write_bytes(0, 1);
        rest.write_bytes(0, 1);
        let [a, b, c] = &mut *rest;
        [&mut *pro, &mut *app, a, b, c]
    }
}

#[embassy_executor::task(pool_size = HTTPS_TASKS)]
async fn https_task(stack: Stack<'static>, buffers: &'static mut TcpBuffers) -> ! {
    use embassy_time::with_timeout;
    loop {
        let mut socket = embassy_net::tcp::TcpSocket::new(stack, &mut buffers.rx, &mut buffers.tx);
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
            crate::heap::boxed(|| serve_session(&mut socket, &mut slot)).await;
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
            let http = &mut buffers.http;
            let out = &mut buffers.out;
            let _ = crate::heap::boxed(|| {
                picoserve::Server::new(&app, &SERVER_CONFIG, http)
                    .serve(TlsConnection::new(read, write, out))
            })
            .await;
        }
        let _ = with_timeout(Duration::from_secs(1), session.close()).await;
    }
}

fn app() -> Router<impl PathRouter> {
    // Specific device resources take precedence over the command subtree.
    let api = commands::routes(Router::new());
    let api = devices::routes(api);
    let api = mqtt::routes(api);
    let api = sensors::routes(api);
    let api = location::routes(api);
    let api = network::routes(api);
    let api = logs::routes(api);
    let api = settings::routes(api)
        .route("/system", get(system::get_system))
        .route("/system/restart", post(system::restart));
    let api = tls::routes(api);
    let api = firmware::routes(api);
    let api = automation::routes(api);
    // The API outermost: every route a request passes on its way costs a
    // stack frame (see `automation::routes`), and the pages are asked for
    // only when one loads.
    assets::routes(Router::new())
        .nest("/api/v1", api)
        .layer(BasicAuth)
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
            // The route's handler on the heap: the router keeps room for
            // its largest one, and every layer above builds this future on
            // the stack (see `crate::heap`).
            crate::heap::boxed(|| next.run(state, path_parameters, response_writer)).await
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
