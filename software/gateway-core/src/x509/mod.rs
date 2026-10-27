//! X.509 for the web server's HTTPS certificate (ADR 0018): reading
//! certificates, writing the gateway's own CSR and self-signed certificate,
//! and the rules the certificate manager in `gateway-hardware::cert` decides
//! by. All of it pure and host-tested; keys and signing stay in the
//! hardware crate.

pub mod cert;
pub mod check;
pub mod der;
pub mod issue;
pub mod pem;
pub mod pkcs7;
pub mod time;

#[cfg(test)]
pub(crate) mod fixtures;
