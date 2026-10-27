# What the EST server has to support

The gateway can get and renew its HTTPS certificate over EST (RFC 7030),
see [ADR 0018](adr/0018-https-certificate-management.md). It implements only
part of EST, the part it needs. This page lists what a server has to offer
so that the gateway works with it. It describes the gateway's
implementation (`gateway-hardware/src/cert/`, `gateway-core/src/est.rs`,
`gateway-core/src/x509/`); keep it in step with that code.

## In short

- HTTPS on IPv4, TLS 1.2 or 1.3, a certificate that chains to a CA the user
  installs on the gateway as the trust anchor, for the host name in the URL.
- `POST …/simpleenroll` and `POST …/simplereenroll` with a base64 PKCS#10
  request; the answer is a base64 certs-only PKCS#7.
- Client authentication only by TLS client certificate: the device
  identity for the first enrollment (optional), the certificate being
  renewed for a renewal. No HTTP authentication.
- Renewal **keeps the key**: the server must allow a new certificate for
  the same public key.

## Connection

| | |
|---|---|
| URL | `https://host[:port][/path]`, configured on the gateway. Without a path the gateway uses `/.well-known/est`; a path replaces it completely (e.g. `/.well-known/est/gateways` for a CA label). Port 443 unless given. No user name in the URL, no query |
| Address | The host is resolved with a DNS **A** query (IPv4 only), or is an IPv4 address. The server must be reachable over IPv4 |
| TLS versions | TLS 1.2 or TLS 1.3 |
| Cipher suites | TLS 1.3: `TLS_AES_128_GCM_SHA256`, `TLS_AES_256_GCM_SHA384`. TLS 1.2: ECDHE with ECDSA or RSA authentication and AES-GCM (`ECDHE-ECDSA-AES128-GCM-SHA256`, `ECDHE-RSA-AES128-GCM-SHA256` and their AES-256/SHA-384 variants). No ChaCha20, no CBC, no static RSA key exchange |
| Key exchange groups | x25519, secp256r1, secp384r1 |
| Server certificate | ECDSA (P-256, P-384) or RSA key; signed with SHA-256/384/512 (ECDSA, RSA PKCS#1 v1.5 or RSA-PSS). Certificates signed with SHA-1 or MD5, and Ed25519 keys, are not accepted |
| Verification | Always on, can't be switched off: the server's chain must lead to a trust anchor installed on the gateway (the server's root or an intermediate CA, PEM/DER, up to 6 KiB in all), the certificate must name the URL's host (SAN DNS name; use a DNS name in the URL rather than an IP address), and it must be within its validity. The gateway sends the host as SNI |
| Certificate message | The server's whole certificate chain must fit 16 KiB |
| Time | The gateway checks dates against SNTP time and makes no EST request before the time is known, so the gateway's NTP server must be reachable |
| Timeouts | 30 s to connect, 30 s for handshake, request and answer together. The gateway's TLS handshake is done in software and takes a moment |
| Not used | `/cacerts` (the trust anchor is installed by hand), `/csrattrs`, `/serverkeygen`, `/fullcmc`, session resumption, ALPN |

## Authentication

The gateway authenticates **only with a TLS client certificate**. It does not
send HTTP Basic or Digest credentials and doesn't answer a `401`.

| Request | Client certificate the gateway presents |
|---|---|
| First `simpleenroll`, and a new `simpleenroll` after the FQDN changed or after a fallback to self-signed | The **device identity**, if the user installed one on the gateway (certificate chain up to 4 KiB, private key EC or RSA, up to 2 KiB DER). Without one: **no** client certificate |
| `simplereenroll` | The **currently active certificate** the EST server issued, with the device key (ECDSA P-256), as RFC 7030 4.2.2 asks |

So the server has to request a client certificate (TLS
`CertificateRequest`) and trust the CA of the device identity and its own
issuing CA for it. For a first enrollment without an identity, the server
has to accept unauthenticated requests or put them up for approval (`202`,
see below), e.g. manual approval by an administrator.

The CSR doesn't carry the TLS channel binding (`tls-unique` in
`challengePassword`, RFC 7030 3.5): a server that requires it for proof of
possession refuses the gateway.

## The request

```text
POST /.well-known/est/simpleenroll HTTP/1.1
Host: est.example.com[:port]
Content-Type: application/pkcs10
Content-Transfer-Encoding: base64
Content-Length: <n>
Accept: application/pkcs7-mime
Connection: close

<base64 of the DER CSR, 64 characters per line>
```

The CSR (PKCS#10, signed with ECDSA-with-SHA256):

| Field | Content |
|---|---|
| Subject | `CN=<FQDN>`; empty if the FQDN is longer than 64 characters |
| Public key | ECDSA P-256, the gateway's device key. **The same key for every enrollment and renewal**: the key is made once and never changes |
| Requested extensions | Subject alternative name `DNS:<FQDN>` (critical if the subject is empty). Nothing else: no key usage, no extended key usage, no `challengePassword` |

The FQDN is the one configured on the gateway (or its hostname if none is
set): letters, digits and hyphens in dot-separated labels, at most 253
characters.

## The answer

| Status | What the gateway does |
|---|---|
| `200` | Reads the body as **base64** (line breaks allowed) of a certs-only PKCS#7 `SignedData` (`application/pkcs7-mime; smime-type=certs-only`). A binary body is not accepted. The body may come with `Content-Length`, chunked, or delimited by closing the connection |
| `202` | The request awaits approval. The gateway asks again after `Retry-After` seconds (a number; an HTTP date isn't understood and counts as 600 s), at least 60 s later |
| anything else | Counts as a failure; the gateway tries again after 1 minute, doubling the wait each time up to 6 hours, and shows the status in the UI |

The whole response, headers included, may be at most 12 KiB.

### The issued certificate

The gateway installs it only if all of this holds; otherwise it keeps its
current certificate and counts the attempt as failed:

- One certificate in the PKCS#7 carries the gateway's public key (the one
  from the CSR). The gateway builds the chain from it by issuer/subject, in
  whatever order the certificates come; certificates that don't belong to
  it are ignored.
- The chain has at most 5 certificates and at most 6 KiB DER in all (the
  gateway serves it in its own TLS handshakes). Intermediate CAs should be
  included: browsers need them. The root may be left out.
- A SAN DNS name matches the FQDN (case does not matter; a wildcard `*.`
  covers exactly one label). A CN alone is not enough.
- It is valid now: `notBefore` at most 5 minutes in the future, `notAfter`
  in the future.

The gateway doesn't check key usage and extended key usage, but browsers
do: the certificate should have `keyUsage` `digitalSignature` and
`extendedKeyUsage` `serverAuth`. For browsers to accept it without a
warning, it has to chain to a CA that the browsers trust.

## Renewal

With automatic renewal on, the gateway sends `simplereenroll` 30 days
before `notAfter`; for certificates that live less than 90 days, after two
thirds of their lifetime. The request has the same subject, SAN and public
key as the certificate being renewed. Renewing can also be started by hand
in the UI (`POST /api/v1/tls/est/enroll`).

## Checklist for setting up a server

- [ ] Reachable over IPv4 under a DNS name; the server certificate has that
      name as a SAN DNS name and offers one of the cipher suites above.
- [ ] The issuing CA (or its root) is installed on the gateway as trust anchor.
- [ ] `simpleenroll` and `simplereenroll` are offered under the configured path.
- [ ] Answers are base64, certs-only PKCS#7, with the intermediate CAs.
- [ ] Issued certificates carry the requested SAN, `serverAuth` and
      `digitalSignature`.
- [ ] Re-enrollment with an unchanged key is allowed.
- [ ] The first enrollment is authorised by the device identity's CA, by
      approval (`202`), or allowed without authentication.
- [ ] No HTTP authentication and no `tls-unique` proof of possession required.
