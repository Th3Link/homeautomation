//! The web server: a REST API under `/api/v1` and, later, the browser UI
//! that uses it. See `gateway-core/docs/api-rest.md` for the routes.
//!
//! Every route sits behind [`BasicAuth`] (ADR 0008: no bypass, not even for
//! read-only ones). One [`web_task`] serves one connection at a time, so
//! [`TASKS`] of them are started; each builds the (zero-sized) router itself.

mod commands;
mod devices;
mod error;
mod json;
mod mqtt;
mod settings;
mod system;

use crate::config;
use embassy_executor::Spawner;
use embassy_net::Stack;
use embassy_time::Duration;
use gateway_core::web::{credentials_match, decode_basic_auth};
use picoserve::io::Read;
use picoserve::response::{IntoResponse, StatusCode};
use picoserve::routing::{get, post, Layer, Next, PathRouter};
use picoserve::{Config, ResponseSent, Router, Timeouts};

/// How many connections can be served at the same time.
pub const TASKS: usize = 3;
const PORT: u16 = 80;

static SERVER_CONFIG: Config = Config::new(Timeouts {
    start_read_request: Duration::from_secs(5),
    persistent_start_read_request: Duration::from_secs(1),
    read_request: Duration::from_secs(5),
    write: Duration::from_secs(5),
})
.keep_connection_alive();

/// Starts the web server tasks on `stack`.
pub fn spawn(spawner: &Spawner, stack: Stack<'static>) {
    spawner.spawn(system::restart_task().unwrap());
    for id in 0..TASKS {
        spawner.spawn(web_task(id, stack).unwrap());
    }
}

#[embassy_executor::task(pool_size = TASKS)]
async fn web_task(id: usize, stack: Stack<'static>) -> ! {
    // The server's future, with the buffers it holds, is ~12 KB. On the heap
    // (the otherwise unused RAM above the statics) it doesn't take away the
    // stack's room the way the task's static storage would.
    alloc::boxed::Box::pin(serve(id, stack)).await
}

async fn serve(id: usize, stack: Stack<'static>) -> ! {
    let app = app();
    let mut http_buffer = [0u8; 1536];
    let mut tcp_rx = [0u8; 2048];
    let mut tcp_tx = [0u8; 1024];
    picoserve::Server::new(&app, &SERVER_CONFIG, &mut http_buffer)
        .listen_and_serve(id, stack, PORT, &mut tcp_rx, &mut tcp_tx)
        .await
        .into_never()
}

fn app() -> Router<impl PathRouter> {
    let api = Router::new()
        .route("/system", get(system::get_system))
        .route("/system/restart", post(system::restart));
    let api = devices::routes(api);
    let api = commands::routes(api);
    let api = settings::routes(api);
    let api = mqtt::routes(api);
    Router::new().nest("/api/v1", api).layer(BasicAuth)
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
