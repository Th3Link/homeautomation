# ADR 0018: HTTPS for the web UI, with central certificate management

**Status:** Accepted

## Context

The web UI and the REST API ran on plain HTTP: the Basic Auth credentials
and the write-only secrets (MQTT passwords, the web password) crossed the
LAN in clear text. HTTPS needs a TLS *server*; `embedded-tls` (used for
`mqtts://`, ADR 0015) is client-only. A certificate also has to come from
somewhere, and installations differ: some have no infrastructure at all,
some sign by hand with their own CA, some run an EST server (RFC 7030).
The gateway has no clock.

## Decision

**TLS library.** `mbedtls-rs` (esp-rs, MbedTLS 3) for the web server and
the EST client, built from source with the Xtensa GCC: 16 KiB input
records (a browser sends full records, e.g. during a firmware upload),
8 KiB output (the certificate chain has to fit one handshake message).
TLS 1.2 and 1.3, ECDHE with ECDSA or RSA certificates. The MQTT client
stays on `embedded-tls`.

**One TLS session at a time.** A session needs ~40 KB of the 94 KiB heap.
The web server's TLS connections and the EST client take turns
(`https::slot`); further connections are accepted and wait (a browser's
keep-alive connection lets go after a second of idleness).

**Ports.** 443 serves the UI and the API over TLS; 80 answers every request
with `308 Permanent Redirect` to the same path on `https://` (308, unlike
301/302, keeps the method and the body, so API clients' `PUT`s arrive as
`PUT`s). `web-https off` on the console goes back to plain HTTP on port
80: the way in if TLS ever doesn't work.

**Certificate management in one place** (`gateway-hardware::cert`, rules in
`gateway_core::x509`):

- One ECDSA P-256 **device key**, made on the gateway from the hardware
  RNG on first boot, kept in flash, never exported: no route, console
  command or log line shows it. From it and the configured **FQDN** the
  gateway writes its own PKCS#10 CSR (SAN `DNS:<fqdn>`, CN if ≤ 64
  characters) and self-signed certificates.
- Three **modes**: *self-signed* (validity configurable, default 10 years),
  *manual* (download the CSR, upload the signed certificate/chain) and
  *EST* (`simpleenroll` for the first certificate and after an FQDN change,
  `simplereenroll` 30 days before `notAfter`, retries with backoff from one
  minute to six hours). The EST server is always verified against the
  installed **trust anchor** and the URL's host name; there is no switch to
  turn that off. A first enrollment authenticates with an installed device
  identity (mTLS) if there is one; a renewal with the certificate being
  renewed (RFC 7030 4.2.2).
- Every new certificate is **checked completely before it replaces the
  active one**: it parses, carries the device key, names the FQDN in its
  SAN and is within its validity as far as the clock tells. The chain is
  written to the inactive of two flash slots and becomes active with one
  write of a pointer, so a power cut leaves the old or the new one, never a
  mix.
- **Fallback:** without a usable certificate (none, broken, other key,
  expired) the gateway makes a self-signed one. HTTPS always has a
  certificate; a failed EST attempt or a refused upload changes nothing.
- **Time:** SNTP (configurable server, default `pool.ntp.org`). Until it
  answered, "valid" means "not expired before the firmware was built", EST
  waits, and the status says the time is unknown.

## Consequences

- The credentials and secrets are encrypted on the wire. Self-signed
  certificates make browsers warn until the user trusts the certificate;
  the manual and EST modes give certificates the browsers already trust.
- The firmware grows by ~730 KB of flash (MbedTLS) and ~5 KB of static RAM;
  a handshake (software ECDH and ECDSA on a 160 MHz core) takes a moment.
  Browsers that open many connections at once are served one after the
  other.
- Building `gateway-hardware` needs `cmake` and `ninja` besides the esp
  toolchain (MbedTLS is compiled from source).
- The device key and the EST identity's key lie in flash unencrypted, like
  the passwords: see "No secure boot, no flash encryption" in
  `technical-debt.md`.
