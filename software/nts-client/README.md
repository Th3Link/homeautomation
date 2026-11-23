# nts-client

Network Time Security (RFC 8915) for NTPv4 with AES-SIV-CMAC-256.

| Cargo feature | API |
| --- | --- |
| Default | `no_std` wire codec, TLS exporter contexts and fixed-capacity cookie storage |
| `async-net` | `client::async_net` TCP, TLS and UDP via the async-io reactor |
| `smol` | The same client under `client::smol` |
| `tokio` | `client::tokio` with Tokio sockets and timers |

From another Cargo project:

```toml
[dependencies]
nts-client = { git = "https://github.com/Th3Link/homeautomation", features = ["tokio"] }
```

Inside a Tokio runtime with I/O and time enabled:

```rust
use nts_client::{client::{tokio, RootCertStore}, KE_PORT};

async fn server_time(host: &str, ca_der: Vec<u8>) -> std::io::Result<u64> {
    let mut roots = RootCertStore::empty();
    roots.add(ca_der.into()).map_err(std::io::Error::other)?;
    let mut peer = tokio::key_exchange(host, KE_PORT, roots).await?;
    Ok(tokio::query(&mut peer).await?.transmit)
}
```

`transmit` is the server’s 64-bit NTP timestamp. Keep the peer for repeated queries.
A query consumes a cookie. Authenticated replies replenish cookies. When
`peer.cookies() == 0`, repeat key exchange. The smol and async-net modules expose the same functions.

On embedded targets, negotiate TLS 1.3 with ALPN `ntske/1`, verify the server,
send `KE_REQUEST` and feed the reply to `parse_ke_response`. Derive `Keys` with the TLS
exporter. Use `Session::prepare` and `Session::accept` with caller-owned packet
buffers and fresh random identifiers and nonces. The core retains up to eight 192-byte cookies.

Run `cargo test -p nts-client` for the protocol vectors. On Linux,
`software/nts-client/tests/chrony.sh` exercises both host transports against local chrony.
