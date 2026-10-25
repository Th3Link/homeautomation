# ADR 0015: MQTT over TLS 1.3 with a pinned broker key

**Status:** Accepted

## Context

Brokers on the LAN (Home Assistant's, a Cerbo's) are mostly reached over
plain MQTT. Encrypting the connection is wanted, but the gateway has no
clock (so certificate validity can't be checked), brokers on a LAN usually
have self-signed certificates (so there is no authority to check against)
and are addressed by IP or by a name that isn't in the certificate (so the
host name can't be matched). `embedded-tls` is the one `no_std` TLS
library for the target; it is client-only, TLS 1.3 only, and its own
certificate verifier insists on a matching host name.

## Decision

`mqtts://host[:8883]` connects with TLS 1.3 (`TLS_AES_128_GCM_SHA256`) and
the broker is authenticated by its **public key**, pinned by the user:

- `mqtt-trust` makes the next connection capture the certificate the
  broker presents and store it; its SHA-256 fingerprint is logged.
- From then on the certificate must carry that public key, and the
  handshake signature (ECDSA P-256/P-384 or RSA-PSS) must verify with it.
  A renewed certificate with the same key is accepted; a different key
  is refused.
- `mqtt-untrust` forgets the pin, `mqtt-insecure on` accepts any
  certificate (encrypted, not authenticated).
- Validity dates, issuer and host name are not checked.

## Consequences

- No clock and no certificate authority are needed, and certificate
  renewals with an unchanged key go unnoticed, which is what a LAN broker
  wants. A new key needs `mqtt-trust` again (or the REST API's
  `POST /mqtt/:slot/trust`).
- RSA verification needs a heap (the `rsa` crate), so the firmware has
  one: 94 KiB in the otherwise unused `dram2` region of the ESP32.
- The broker has to speak TLS 1.3 and offer that cipher suite; every
  current Mosquitto/OpenSSL does.
- The same TLS code can't be used as an HTTPS server (client-only); that is
  a separate piece of work.
