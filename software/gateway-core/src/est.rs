//! The HTTP side of EST (RFC 7030): where to send a request, what the
//! request looks like, and what to make of the answer. The TLS connection
//! (always verified against the trust anchor) is `gateway-hardware::cert`'s.

use core::fmt;

/// An EST server: `https://host[:port][/path]`. Without a path the
/// well-known one is used; a path is the server's EST base, e.g.
/// `/.well-known/est/arbitrary-label`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Server<'a> {
    pub host: &'a str,
    pub port: u16,
    pub base_path: &'a str,
}

pub const WELL_KNOWN: &str = "/.well-known/est";

/// Parses an EST server URL; only `https://` is accepted.
pub fn parse_url(url: &str) -> Option<Server<'_>> {
    let rest = url.trim().strip_prefix("https://")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].trim_end_matches('/')),
        None => (rest, ""),
    };
    if authority.contains('@') {
        return None;
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, port.parse().ok().filter(|p| *p != 0)?),
        None => (authority, 443),
    };
    if host.is_empty()
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
    {
        return None;
    }
    if path.contains(['?', '#', ' ']) {
        return None;
    }
    Some(Server {
        host,
        port,
        base_path: if path.is_empty() { WELL_KNOWN } else { path },
    })
}

/// The two operations the gateway uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    /// First enrollment (RFC 7030 4.2.1).
    Enroll,
    /// Renewal of the certificate the client authenticates with (4.2.2).
    Reenroll,
}

impl Operation {
    pub fn path(self) -> &'static str {
        match self {
            Operation::Enroll => "simpleenroll",
            Operation::Reenroll => "simplereenroll",
        }
    }
}

/// The request head; `body_len` is the length of the base64 body (the
/// CSR) that follows it.
pub struct RequestHead<'a> {
    pub server: Server<'a>,
    pub operation: Operation,
    pub body_len: usize,
}

impl fmt::Display for RequestHead<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "POST {}/{} HTTP/1.1\r\nHost: {}",
            self.server.base_path,
            self.operation.path(),
            self.server.host
        )?;
        if self.server.port != 443 {
            write!(f, ":{}", self.server.port)?;
        }
        write!(
            f,
            "\r\nContent-Type: application/pkcs10\r\n\
             Content-Transfer-Encoding: base64\r\n\
             Content-Length: {}\r\n\
             Accept: application/pkcs7-mime\r\n\
             Connection: close\r\n\r\n",
            self.body_len
        )
    }
}

/// What an EST server answered.
#[derive(Debug, PartialEq, Eq)]
pub enum Answer<'a> {
    /// 200: the base64 of a certs-only PKCS#7 holding the certificate.
    Issued(&'a [u8]),
    /// 202: the request awaits approval; ask again after that many seconds.
    Pending { retry_after: u32 },
    /// Any other status.
    Refused { status: u16 },
}

/// The response isn't HTTP the gateway can read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Malformed;

fn header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.split("\r\n").skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

/// Reads a complete response (the connection was closed by the server).
/// A chunked body is joined in place, which is why `response` is mutable.
pub fn parse_response(response: &mut [u8]) -> Result<Answer<'_>, Malformed> {
    let head_end = response
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or(Malformed)?;
    let head = core::str::from_utf8(&response[..head_end]).map_err(|_| Malformed)?;
    let mut status_line = head.split("\r\n").next().unwrap_or("").split(' ');
    if !status_line.next().unwrap_or("").starts_with("HTTP/1.") {
        return Err(Malformed);
    }
    let status: u16 = status_line
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or(Malformed)?;
    let chunked =
        header(head, "Transfer-Encoding").is_some_and(|v| contains_ignore_case(v, "chunked"));
    let length = header(head, "Content-Length").and_then(|v| v.parse::<usize>().ok());
    let retry_after = header(head, "Retry-After").and_then(|v| v.parse::<u32>().ok());

    match status {
        200 => {}
        202 => {
            return Ok(Answer::Pending {
                retry_after: retry_after.unwrap_or(600),
            })
        }
        _ => return Ok(Answer::Refused { status }),
    }

    let body_start = head_end + 4;
    let body = &mut response[body_start..];
    let len = if chunked {
        dechunk(body)?
    } else {
        match length {
            Some(n) if n <= body.len() => n,
            Some(_) => return Err(Malformed),
            None => body.len(),
        }
    };
    Ok(Answer::Issued(&response[body_start..body_start + len]))
}

fn contains_ignore_case(haystack: &str, needle: &str) -> bool {
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
}

/// Joins a chunked body in place; returns its length.
fn dechunk(body: &mut [u8]) -> Result<usize, Malformed> {
    let mut read = 0;
    let mut write = 0;
    loop {
        let line_end = body[read..]
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or(Malformed)?
            + read;
        let size_text = core::str::from_utf8(&body[read..line_end]).map_err(|_| Malformed)?;
        let size_text = size_text.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_text, 16).map_err(|_| Malformed)?;
        read = line_end + 2;
        if size == 0 {
            return Ok(write);
        }
        if read + size + 2 > body.len() {
            return Err(Malformed);
        }
        body.copy_within(read..read + size, write);
        write += size;
        read += size + 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::string::ToString;
    use std::vec::Vec;

    #[test]
    fn urls() {
        assert_eq!(
            parse_url("https://est.example.com"),
            Some(Server {
                host: "est.example.com",
                port: 443,
                base_path: WELL_KNOWN
            })
        );
        assert_eq!(
            parse_url("https://est.example.com:8443/.well-known/est/gateways/"),
            Some(Server {
                host: "est.example.com",
                port: 8443,
                base_path: "/.well-known/est/gateways"
            })
        );
        assert_eq!(parse_url("http://est.example.com"), None);
        assert_eq!(parse_url("https://"), None);
        assert_eq!(parse_url("https://user@est.example.com"), None);
        assert_eq!(parse_url("https://est.example.com:0"), None);
        assert_eq!(parse_url("https://est.example.com/x?y"), None);
    }

    #[test]
    fn request_head() {
        let head = RequestHead {
            server: parse_url("https://est.example.com:8443").unwrap(),
            operation: Operation::Reenroll,
            body_len: 600,
        }
        .to_string();
        assert!(head.starts_with("POST /.well-known/est/simplereenroll HTTP/1.1\r\n"));
        assert!(head.contains("Host: est.example.com:8443\r\n"));
        assert!(head.contains("Content-Length: 600\r\n"));
        assert!(head.contains("Content-Type: application/pkcs10\r\n"));
        assert!(head.ends_with("\r\n\r\n"));
    }

    fn answer(raw: &str) -> Result<Vec<u8>, Malformed> {
        let mut buf = raw.as_bytes().to_vec();
        match parse_response(&mut buf)? {
            Answer::Issued(body) => Ok(body.to_vec()),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn issued_plain_and_chunked() {
        let plain = "HTTP/1.1 200 OK\r\nContent-Type: application/pkcs7-mime\r\nContent-Length: 4\r\n\r\nMIIB";
        assert_eq!(answer(plain).unwrap(), b"MIIB");
        let no_length = "HTTP/1.0 200 OK\r\n\r\nMIIB\r\n";
        assert_eq!(answer(no_length).unwrap(), b"MIIB\r\n");
        let chunked =
            "HTTP/1.1 200 OK\r\ntransfer-encoding: Chunked\r\n\r\n4\r\nMIIB\r\n3;x=y\r\nabc\r\n0\r\n\r\n";
        assert_eq!(answer(chunked).unwrap(), b"MIIBabc");
    }

    #[test]
    fn pending_refused_and_garbage() {
        let mut buf = b"HTTP/1.1 202 Accepted\r\nRetry-After: 120\r\n\r\n".to_vec();
        assert_eq!(
            parse_response(&mut buf),
            Ok(Answer::Pending { retry_after: 120 })
        );
        let mut buf = b"HTTP/1.1 401 Unauthorized\r\n\r\nno".to_vec();
        assert_eq!(
            parse_response(&mut buf),
            Ok(Answer::Refused { status: 401 })
        );
        let mut buf = b"SSH-2.0\r\n\r\n".to_vec();
        assert_eq!(parse_response(&mut buf), Err(Malformed));
        let mut buf = b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nshort".to_vec();
        assert_eq!(parse_response(&mut buf), Err(Malformed));
        let mut buf = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nZZ\r\n".to_vec();
        assert_eq!(parse_response(&mut buf), Err(Malformed));
        let mut buf = b"HTTP/1.1 200 OK".to_vec();
        assert_eq!(parse_response(&mut buf), Err(Malformed));
    }
}
