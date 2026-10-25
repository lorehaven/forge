//! A deliberately dumb HTTP/1.1 client that writes exactly the bytes it is told to.
//!
//! It does no normalisation, so `/../`, `%00` and malformed framing reach the server as written. A
//! normal client would clean those up before the server ever saw them, defeating the test.

use quench_proto::tls::{ClientOptions, TlsConnector, client_config, server_name};
use std::net::IpAddr;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;

/// Replies larger than this are cut off; a scanner has no use for more.
const MAX_REPLY: usize = 1 << 20;

#[derive(Debug, Clone)]
pub struct Target {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub tls: bool,
    /// Prepended to every route (the service's `BASE_PATH`), without a trailing slash.
    pub base_path: String,
    /// Skip certificate verification (self-signed staging certificates).
    pub insecure: bool,
}

impl Target {
    /// Parses `http://host:port/base` or `https://host/base`.
    ///
    /// # Errors
    ///
    /// A description of what is wrong with the URL.
    pub fn parse(name: &str, url: &str) -> Result<Self, String> {
        let (tls, rest) = if let Some(rest) = url.strip_prefix("https://") {
            (true, rest)
        } else if let Some(rest) = url.strip_prefix("http://") {
            (false, rest)
        } else {
            return Err(format!("{url}: expected an http:// or https:// URL"));
        };
        let (authority, path) = rest.split_once('/').map_or((rest, ""), |(a, p)| (a, p));
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) if !h.ends_with(']') || p.chars().all(|c| c.is_ascii_digit()) => (
                h.trim_matches(['[', ']']),
                p.parse().map_err(|_| format!("{url}: bad port"))?,
            ),
            _ => (
                authority.trim_matches(['[', ']']),
                if tls { 443 } else { 80 },
            ),
        };
        if host.is_empty() {
            return Err(format!("{url}: no host"));
        }
        Ok(Self {
            name: name.to_string(),
            host: host.to_string(),
            port,
            tls,
            base_path: format!("/{}", path.trim_matches('/'))
                .trim_end_matches('/')
                .to_string(),
            insecure: false,
        })
    }

    /// Loopback, private-range and cluster-internal hosts: places a pentest of one's own estate is
    /// expected to point. Anything else needs an explicit `--allow-remote`.
    #[must_use]
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    pub fn is_private(&self) -> bool {
        let host = self.host.to_ascii_lowercase();
        if host == "localhost"
            || host.ends_with(".localhost")
            || host.ends_with(".svc")
            || host.ends_with(".cluster.local")
        {
            return true;
        }
        match host.parse::<IpAddr>() {
            Ok(IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
            Ok(IpAddr::V6(ip)) => ip.is_loopback(),
            Err(_) => false,
        }
    }

    #[must_use]
    pub fn url_for(&self, path: &str) -> String {
        format!("{}{path}", self.base_path)
    }
}

#[derive(Debug, Clone)]
pub struct Probe {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Probe {
    #[must_use]
    pub fn new(method: &str, path: &str) -> Self {
        Self {
            method: method.to_string(),
            path: path.to_string(),
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    #[must_use]
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    #[must_use]
    pub fn bearer(self, token: &str) -> Self {
        self.header("Authorization", &format!("Bearer {token}"))
    }

    #[must_use]
    pub fn json(mut self, body: &str) -> Self {
        self.body = body.as_bytes().to_vec();
        self.header("Content-Type", "application/json")
    }

    fn to_bytes(&self, target: &Target) -> Vec<u8> {
        let mut out = format!(
            "{} {} HTTP/1.1\r\n",
            self.method,
            target.url_for(&self.path)
        )
        .into_bytes();
        let has = |name: &str| {
            self.headers
                .iter()
                .any(|(n, _)| n.eq_ignore_ascii_case(name))
        };
        if !has("host") {
            out.extend_from_slice(format!("Host: {}:{}\r\n", target.host, target.port).as_bytes());
        }
        if !has("user-agent") {
            out.extend_from_slice(b"User-Agent: forge-sectest/0.1 (authorised security test)\r\n");
        }
        out.extend_from_slice(b"Connection: close\r\nAccept: */*\r\n");
        for (name, value) in &self.headers {
            out.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
        }
        if !self.body.is_empty() || matches!(self.method.as_str(), "POST" | "PUT" | "PATCH") {
            out.extend_from_slice(format!("Content-Length: {}\r\n", self.body.len()).as_bytes());
        }
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(&self.body);
        out
    }
}

#[derive(Debug, Clone, Default)]
pub struct Reply {
    pub status: u16,
    /// Lower-cased names.
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub elapsed: Duration,
}

impl Reply {
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    #[must_use]
    pub fn all(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .collect()
    }

    #[must_use]
    pub fn is_html(&self) -> bool {
        self.header("content-type")
            .is_some_and(|v| v.to_ascii_lowercase().contains("text/html"))
    }

    #[must_use]
    pub const fn denied(&self) -> bool {
        matches!(self.status, 401 | 403)
    }
}

/// What came back on a raw connection.
#[derive(Debug, Clone, Default)]
pub struct RawReply {
    pub data: Vec<u8>,
    /// The server closed the connection (as opposed to us giving up waiting).
    pub closed: bool,
    pub elapsed: Duration,
}

impl RawReply {
    #[must_use]
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.data).into_owned()
    }

    /// How many HTTP responses the server sent.
    #[must_use]
    pub fn responses(&self) -> usize {
        self.text().matches("HTTP/1.").count()
    }

    #[must_use]
    pub fn first_status(&self) -> Option<u16> {
        self.text().strip_prefix("HTTP/1.")?.get(2..5)?.parse().ok()
    }
}

async fn drive<S>(mut stream: S, bytes: &[u8], wait: Duration) -> RawReply
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let started = Instant::now();
    // A server may hang up before reading everything (that is often the defence working).
    let _ = stream.write_all(bytes).await;
    let mut data = Vec::new();
    let mut buf = [0u8; 8192];
    let mut closed = false;
    let deadline = started + wait;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        match tokio::time::timeout(left, stream.read(&mut buf)).await {
            Ok(Ok(0) | Err(_)) => {
                closed = true;
                break;
            }
            Ok(Ok(n)) => {
                data.extend_from_slice(&buf[..n]);
                if data.len() >= MAX_REPLY {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    RawReply {
        data,
        closed,
        elapsed: started.elapsed(),
    }
}

/// Writes `bytes` verbatim on a fresh connection and returns whatever comes back within `wait`.
///
/// # Errors
///
/// When the connection cannot be made.
pub async fn raw(target: &Target, bytes: &[u8], wait: Duration) -> Result<RawReply, String> {
    let tcp = tokio::time::timeout(
        Duration::from_secs(10),
        TcpStream::connect((target.host.as_str(), target.port)),
    )
    .await
    .map_err(|_| format!("{}:{}: connect timed out", target.host, target.port))?
    .map_err(|e| format!("{}:{}: {e}", target.host, target.port))?;
    if !target.tls {
        return Ok(drive(tcp, bytes, wait).await);
    }
    let options = ClientOptions {
        accept_invalid_certs: target.insecure,
        use_system_roots: true,
        ..ClientOptions::default()
    };
    let connector = TlsConnector::new(client_config(&options)?);
    let stream = connector
        .connect(server_name(&target.host)?, tcp)
        .await
        .map_err(|e| format!("TLS handshake with {}: {e}", target.host))?;
    Ok(drive(stream, bytes, wait).await)
}

/// Sends one probe and parses the reply.
///
/// # Errors
///
/// When the connection fails or the server sends nothing parseable.
pub async fn send(target: &Target, probe: &Probe) -> Result<Reply, String> {
    let raw = raw(target, &probe.to_bytes(target), Duration::from_secs(15)).await?;
    parse_reply(&raw.data).map(|mut r| {
        r.elapsed = raw.elapsed;
        r
    })
}

fn parse_reply(data: &[u8]) -> Result<Reply, String> {
    let split = data
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| "no HTTP response head".to_string())?;
    let head = String::from_utf8_lossy(&data[..split]);
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| "unparseable status line".to_string())?;
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    Ok(Reply {
        status,
        headers,
        body: String::from_utf8_lossy(&data[split + 4..]).into_owned(),
        elapsed: Duration::ZERO,
    })
}
