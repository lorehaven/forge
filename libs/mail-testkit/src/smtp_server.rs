//! A small SMTP submission server that keeps what it is given.

use crate::io::{Conn, Stream, acceptor};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

/// How the server behaves.
#[derive(Debug, Clone)]
pub struct SmtpOptions {
    pub implicit_tls: bool,
    pub starttls: bool,
    /// Refuse MAIL FROM until the client has logged in.
    pub require_auth: bool,
    /// Recipients answered with `550` (the exact address, case-insensitive).
    pub reject_recipients: Vec<String>,
    /// Largest accepted message in bytes (0 = unlimited).
    pub max_size: usize,
}

impl Default for SmtpOptions {
    fn default() -> Self {
        Self {
            implicit_tls: true,
            starttls: false,
            require_auth: true,
            reject_recipients: Vec::new(),
            max_size: 0,
        }
    }
}

/// A message the server accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Received {
    pub auth_user: Option<String>,
    pub from: String,
    pub to: Vec<String>,
    /// The message as sent, dot-unstuffed, with CRLF line endings.
    pub data: Vec<u8>,
}

impl Received {
    /// The message as text (lossy), for assertions.
    #[must_use]
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.data).into_owned()
    }

    /// The value of the first header with this name.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<String> {
        let text = self.text();
        let head = text.split("\r\n\r\n").next().unwrap_or("");
        let mut found: Option<String> = None;
        for line in head.lines() {
            if let Some(value) = &mut found {
                if line.starts_with([' ', '\t']) {
                    value.push(' ');
                    value.push_str(line.trim());
                    continue;
                }
                return found;
            }
            if let Some((n, v)) = line.split_once(':')
                && n.trim().eq_ignore_ascii_case(name)
            {
                found = Some(v.trim().to_string());
            }
        }
        found
    }
}

#[derive(Debug, Default)]
struct State {
    users: Vec<(String, String)>,
    received: Vec<Received>,
    log: Vec<String>,
    /// Answers for the next DATA commands (consumed in order): `(code, text)`.
    data_failures: Vec<(u16, String)>,
    failed_logins: u32,
}

/// A running server. Dropping it stops accepting.
#[derive(Debug)]
pub struct SmtpServer {
    addr: SocketAddr,
    state: Arc<Mutex<State>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for SmtpServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl SmtpServer {
    /// Starts on a free local port.
    ///
    /// # Panics
    ///
    /// When no port can be bound.
    pub async fn start(options: SmtpOptions) -> Self {
        Self::start_on(0, options).await
    }

    /// Starts on `port` (0 for a free one).
    ///
    /// # Panics
    ///
    /// When the port cannot be bound.
    pub async fn start_on(port: u16, options: SmtpOptions) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", port)).await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let state = Arc::new(Mutex::new(State::default()));
        let shared = state.clone();
        let task = tokio::spawn(async move {
            loop {
                let Ok((tcp, _)) = listener.accept().await else {
                    return;
                };
                let state = shared.clone();
                let options = options.clone();
                tokio::spawn(async move {
                    let stream = if options.implicit_tls {
                        match acceptor().accept(tcp).await {
                            Ok(tls) => Stream::Tls(Box::new(tls)),
                            Err(_) => return,
                        }
                    } else {
                        Stream::Plain(tcp)
                    };
                    session(Conn::new(stream), state, options).await;
                });
            }
        });
        Self { addr, state, task }
    }

    #[must_use]
    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// Adds a login the server accepts.
    pub fn add_user(&self, user: &str, password: &str) {
        self.state
            .lock()
            .unwrap()
            .users
            .push((user.to_string(), password.to_string()));
    }

    /// What has been accepted so far.
    #[must_use]
    pub fn received(&self) -> Vec<Received> {
        self.state.lock().unwrap().received.clone()
    }

    /// The commands received, in order (not the message bodies; passwords are masked).
    #[must_use]
    pub fn log(&self) -> Vec<String> {
        self.state.lock().unwrap().log.clone()
    }

    /// Makes the next `DATA` end answer with this reply (`451 try later`), once.
    pub fn fail_next_data(&self, code: u16, text: &str) {
        self.state
            .lock()
            .unwrap()
            .data_failures
            .push((code, text.to_string()));
    }

    /// Refused logins so far.
    #[must_use]
    pub fn failed_logins(&self) -> u32 {
        self.state.lock().unwrap().failed_logins
    }

    /// Forgets what was received.
    pub fn clear(&self) {
        self.state.lock().unwrap().received.clear();
    }
}

fn decode_base64(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for b in text.bytes() {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xFF) as u8);
        }
    }
    out
}

async fn session(mut conn: Conn, state: Arc<Mutex<State>>, options: SmtpOptions) {
    if !conn.send(b"220 mail-testkit ESMTP ready\r\n").await {
        return;
    }
    let mut auth_user: Option<String> = None;
    let mut from = String::new();
    let mut to: Vec<String> = Vec::new();
    while let Some(line) = conn.line().await {
        let text = String::from_utf8_lossy(&line).trim_end().to_string();
        let upper = text.to_ascii_uppercase();
        let masked = if upper.starts_with("AUTH") {
            "AUTH ...".to_string()
        } else {
            text.clone()
        };
        state.lock().unwrap().log.push(masked);
        let reply: String = if upper.starts_with("EHLO") || upper.starts_with("HELO") {
            let mut r = "250-mail-testkit\r\n250-8BITMIME\r\n".to_string();
            if options.max_size > 0 {
                r.push_str(&format!("250-SIZE {}\r\n", options.max_size));
            }
            if options.starttls && !conn.is_tls() {
                r.push_str("250-STARTTLS\r\n");
            }
            if conn.is_tls() || (!options.starttls && !options.implicit_tls) {
                r.push_str("250-AUTH PLAIN LOGIN\r\n");
            }
            r.push_str("250 OK\r\n");
            r
        } else if upper == "STARTTLS" {
            if !options.starttls || conn.is_tls() {
                "503 no STARTTLS\r\n".to_string()
            } else {
                if !conn.send(b"220 go ahead\r\n").await || !conn.upgrade(&acceptor()).await {
                    return;
                }
                from.clear();
                to.clear();
                continue;
            }
        } else if upper.starts_with("AUTH PLAIN") {
            let token = if text.len() > 11 {
                text[11..].trim().to_string()
            } else {
                if !conn.send(b"334 \r\n").await {
                    return;
                }
                let Some(l) = conn.line().await else { return };
                String::from_utf8_lossy(&l).trim().to_string()
            };
            let decoded = decode_base64(&token);
            let mut parts = decoded.split(|b| *b == 0);
            let _ = parts.next();
            let user = String::from_utf8_lossy(parts.next().unwrap_or_default()).into_owned();
            let pass = String::from_utf8_lossy(parts.next().unwrap_or_default()).into_owned();
            login(&state, &mut auth_user, &user, &pass)
        } else if upper == "AUTH LOGIN" {
            if !conn.send(b"334 VXNlcm5hbWU6\r\n").await {
                return;
            }
            let Some(u) = conn.line().await else { return };
            if !conn.send(b"334 UGFzc3dvcmQ6\r\n").await {
                return;
            }
            let Some(p) = conn.line().await else { return };
            let user = String::from_utf8_lossy(&decode_base64(String::from_utf8_lossy(&u).trim()))
                .into_owned();
            let pass = String::from_utf8_lossy(&decode_base64(String::from_utf8_lossy(&p).trim()))
                .into_owned();
            login(&state, &mut auth_user, &user, &pass)
        } else if upper.starts_with("MAIL FROM:") {
            if options.require_auth && auth_user.is_none() {
                "530 5.7.0 authentication required\r\n".to_string()
            } else {
                from = text[10..]
                    .trim()
                    .split(['>', ' '])
                    .next()
                    .unwrap_or("")
                    .trim_start_matches('<')
                    .to_string();
                to.clear();
                "250 OK\r\n".to_string()
            }
        } else if upper.starts_with("RCPT TO:") {
            let rcpt = text[8..]
                .trim()
                .split(['>', ' '])
                .next()
                .unwrap_or("")
                .trim_start_matches('<')
                .to_string();
            if options
                .reject_recipients
                .iter()
                .any(|r| r.eq_ignore_ascii_case(&rcpt))
            {
                "550 5.1.1 no such user\r\n".to_string()
            } else {
                to.push(rcpt);
                "250 OK\r\n".to_string()
            }
        } else if upper == "DATA" {
            if to.is_empty() {
                "503 need RCPT first\r\n".to_string()
            } else {
                if !conn.send(b"354 end with <CRLF>.<CRLF>\r\n").await {
                    return;
                }
                let mut data = Vec::new();
                loop {
                    let Some(l) = conn.line().await else { return };
                    if l == b".\r\n" || l == b".\n" {
                        break;
                    }
                    let l = l
                        .strip_prefix(b".")
                        .filter(|_| l.starts_with(b".."))
                        .map_or(l.clone(), <[u8]>::to_vec);
                    data.extend_from_slice(&l);
                }
                let failure = {
                    let mut s = state.lock().unwrap();
                    if s.data_failures.is_empty() {
                        None
                    } else {
                        Some(s.data_failures.remove(0))
                    }
                };
                if let Some((code, message)) = failure {
                    format!("{code} {message}\r\n")
                } else if options.max_size > 0 && data.len() > options.max_size {
                    "552 5.3.4 message too large\r\n".to_string()
                } else {
                    state.lock().unwrap().received.push(Received {
                        auth_user: auth_user.clone(),
                        from: from.clone(),
                        to: std::mem::take(&mut to),
                        data,
                    });
                    "250 OK queued\r\n".to_string()
                }
            }
        } else if upper == "RSET" {
            from.clear();
            to.clear();
            "250 OK\r\n".to_string()
        } else if upper == "NOOP" {
            "250 OK\r\n".to_string()
        } else if upper == "QUIT" {
            let _ = conn.send(b"221 bye\r\n").await;
            return;
        } else {
            "502 not implemented\r\n".to_string()
        };
        if !conn.send(reply.as_bytes()).await {
            return;
        }
    }
}

fn login(
    state: &Arc<Mutex<State>>,
    auth_user: &mut Option<String>,
    user: &str,
    pass: &str,
) -> String {
    let mut s = state.lock().unwrap();
    if s.users.iter().any(|(u, p)| u == user && p == pass) {
        *auth_user = Some(user.to_string());
        "235 2.7.0 authenticated\r\n".to_string()
    } else {
        s.failed_logins += 1;
        "535 5.7.8 authentication failed\r\n".to_string()
    }
}
