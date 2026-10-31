# ADR 0020: Trusted certificate authorities, Let's Encrypt's built in

**Status:** Accepted

## Context

The gateway checked certificates in two separate ways. The EST client
trusted only the CA certificates the user installed (ADR 0018). The MQTT
client pinned each broker's public key (ADR 0015), because it had no clock
and LAN brokers mostly had self-signed certificates.

Both premises have changed. The gateway has the time from SNTP. And many
home networks have certificates from Let's Encrypt for their brokers and
servers, through cert-manager or certbot. Those renew every 60-90 days and
by default with a **new key** (cert-manager's `rotationPolicy: Always`,
certbot without `--reuse-key`). A pinned key then breaks at every renewal,
and the user has to run `mqtt-trust` again.

## Decision

**One set of trusted CAs** (`gateway-hardware/src/cert/roots.rs`), used by
the EST client and by MQTT:

- **Built in:** Let's Encrypt's roots ISRG Root X1 (RSA), X2 (ECDSA), and
  Root YE and Root YR, the roots of the hierarchy Let's Encrypt issues from
  now (cross-signed by X2 and X1). They are 3.7 KB of flash
  (`gateway-hardware/certs/`, with their fingerprints). They are trusted
  unless switched off (`tls-built-in-roots off`, `PATCH /tls`
  `built_in_roots`, the HTTPS page).
- **Installed:** the CA certificates the user uploads, as before for EST
  (up to 6 KiB).
- No full browser bundle: it would have to be kept up to date with every
  firmware release, and trusting about 150 CAs where one is needed gives
  more than it takes. Another CA is installed by hand.

**MQTT checks a broker in one of three ways:**

| | When | What is checked |
|---|---|---|
| Pinned key | a key is pinned (`mqtt-trust`) | the broker's key, as in ADR 0015 |
| CA | no key pinned (new) | the chain leads to a trusted CA, the leaf names the host of the broker URI (SAN DNS name, or SAN IP address for an address), every certificate is valid now, the leaf allows `digitalSignature` and `serverAuth`; and the handshake signature |
| Any | `mqtt-insecure on` | nothing |

Before, a broker without a pinned key wasn't connected to at all; now it
is checked against the CAs. A pinned key still wins, so nothing changes
for a broker that has one. In CA mode the host name is sent as SNI (not an
address), and the gateway waits for the time from SNTP before connecting.

**The chain is checked by MbedTLS** (`mbedtls_x509_crt_verify`, in
`gateway-hardware/src/tls.rs`), which the firmware already has for HTTPS
and EST. There is no path validation of our own. The connection itself
stays on `embedded-tls` with its static buffers: a second and third
MbedTLS session for the two brokers wouldn't fit the heap.

## Consequences

- A broker or EST server with a Let's Encrypt certificate works without any
  setup on the gateway; renewals with a new key go unnoticed.
- In CA mode the broker URI has to use the name in the certificate (or an
  address that is in it as a SAN IP address).
- While the certificates are checked, MbedTLS' parsed view of the chain
  and of the CAs is on the heap (a few KB; the certificates themselves
  aren't copied). The check runs inside the handshake and blocks the
  executor while it lasts: a P-384 signature takes a moment on this CPU.
- The built-in roots are part of the firmware: a new Let's Encrypt root
  needs a firmware update (or installing it by hand). X1 is trusted until
  2030-2035, X2 until 2035-2040.
- Revocation isn't checked (no CRL, no OCSP), as before for EST.
- ADR 0015 still holds for brokers with a pinned key.
