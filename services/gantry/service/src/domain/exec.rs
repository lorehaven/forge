//! A shell in a pod.
//!
//! The browser cannot hold a socket open through this app's host, so the service does: it opens the
//! Kubernetes exec WebSocket itself and keeps what the pod printed in a bounded buffer that the page reads from
//! (`since` an offset) and writes to (a line at a time). A session is the person's, for as long as they use it.
//!
//! Off unless `GANTRY_EXEC_NAMESPACES` names namespaces, and then only for pods in those.

use quench_k8s::{Auth, Config};
use quench_proto::ws::{self, Message};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

/// What a pod prints is kept this far back; older output is dropped, and the reader is told where it resumes.
const BUFFER_BYTES: usize = 256 * 1024;
/// A session nobody has looked at for this long is closed.
const IDLE: Duration = Duration::from_secs(600);
/// Open sessions, in all: a shell is a pod process, not something to leave piling up.
const MAX_SESSIONS: usize = 8;
const CHANNEL_STDIN: u8 = 0;
const CHANNEL_STATUS: u8 = 3;

/// What comes back from a pod.
#[derive(Debug)]
pub enum ExecEvent {
    Output(Vec<u8>),
    /// The shell ended, or the connection did; the text says why when it is known.
    Closed(Option<String>),
}

/// The two ends of a shell in a pod.
pub struct ExecConnection {
    pub input: mpsc::Sender<Vec<u8>>,
    pub output: mpsc::Receiver<ExecEvent>,
}

impl std::fmt::Debug for ExecConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ExecConnection")
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

// ---------------------------------------------------------------- the pod's end

/// `host:port` of the API server, and whether it is `https`.
fn endpoint(server: &str) -> Result<(String, u16, bool), String> {
    let (tls, rest) = match server.split_once("://") {
        Some(("https", rest)) => (true, rest),
        Some(("http", rest)) => (false, rest),
        _ => return Err(format!("not a server address: {server}")),
    };
    let authority = rest.split('/').next().unwrap_or(rest);
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !host.ends_with(']') || authority.contains("]:") => (
            host.to_string(),
            port.parse::<u16>()
                .map_err(|_| format!("bad port in {server}"))?,
        ),
        _ => (authority.to_string(), if tls { 443 } else { 80 }),
    };
    Ok((host, port, tls))
}

/// The request that asks the API server for a shell, as an upgrade to a WebSocket.
fn upgrade_request(
    host: &str,
    namespace: &str,
    pod: &str,
    container: Option<&str>,
    key: &str,
    bearer: Option<&str>,
) -> String {
    let mut query = String::from("stdin=true&stdout=true&stderr=true&tty=true&command=%2Fbin%2Fsh");
    if let Some(container) = container.filter(|c| !c.is_empty()) {
        query.push_str("&container=");
        query.push_str(&percent(container));
    }
    let mut request = format!(
        "GET /api/v1/namespaces/{}/pods/{}/exec?{query} HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Protocol: v4.channel.k8s.io\r\n",
        percent(namespace),
        percent(pod)
    );
    if let Some(token) = bearer {
        let _ = write!(request, "Authorization: Bearer {token}\r\n");
    }
    request.push_str("\r\n");
    request
}

fn percent(text: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// Reads the response head byte by byte, so nothing past it (the first frames) is consumed.
async fn read_head<I: tokio::io::AsyncRead + Unpin>(io: &mut I) -> Result<String, String> {
    let mut head = Vec::new();
    let mut byte = [0_u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() > 16 * 1024 {
            return Err("the API server's answer has no end".into());
        }
        io.read_exact(&mut byte).await.map_err(|e| e.to_string())?;
        head.push(byte[0]);
    }
    Ok(String::from_utf8_lossy(&head).into_owned())
}

/// A frame of the exec protocol as an event: channels 1 and 2 are the screen, 3 says how it ended.
fn event_of(frame: &[u8]) -> Option<ExecEvent> {
    let (channel, data) = frame.split_first()?;
    match *channel {
        1 | 2 if !data.is_empty() => Some(ExecEvent::Output(data.to_vec())),
        CHANNEL_STATUS => {
            let text = String::from_utf8_lossy(data).into_owned();
            let ended = quench_serde::json::from_str::<quench_serde::Value>(&text)
                .ok()
                .and_then(|v| {
                    v.get("status")
                        .and_then(quench_serde::Value::as_str)
                        .map(str::to_string)
                });
            // `Success` is the shell ending as it should; anything else carries a reason.
            Some(ExecEvent::Closed(match ended.as_deref() {
                Some("Success") | None => None,
                Some(_) => Some(text),
            }))
        }
        _ => None,
    }
}

/// Opens a shell in a pod over the Kubernetes exec WebSocket.
///
/// # Errors
///
/// Why it could not be opened: the API server's address, TLS, or its refusal.
pub async fn connect(
    config: &Config,
    namespace: &str,
    pod: &str,
    container: Option<&str>,
) -> Result<ExecConnection, String> {
    let (host, port, tls) = endpoint(&config.server)?;
    let bearer = match &config.auth {
        Auth::Bearer(token) => Some(token.clone()),
        Auth::BearerFile(path) => Some(
            std::fs::read_to_string(path)
                .map_err(|e| format!("cannot read the token file: {e}"))?
                .trim()
                .to_string(),
        ),
        _ => None,
    };
    let key = ws::generate_key();
    let request = upgrade_request(&host, namespace, pod, container, &key, bearer.as_deref());

    let tcp = tokio::net::TcpStream::connect((host.trim_matches(['[', ']']), port))
        .await
        .map_err(|e| format!("cannot reach the API server: {e}"))?;
    if tls {
        quench_tls::install_default_provider();
        let options = quench_tls::ClientOptions {
            accept_invalid_certs: config.insecure,
            extra_roots_pem: config.ca_pem.clone().into_iter().collect(),
            use_system_roots: true,
            alpn_protocols: vec![b"http/1.1".to_vec()],
        };
        let tls_config = match &config.auth {
            Auth::ClientCertificate { certificate, key } => {
                quench_tls::client_config_with_identity(&options, certificate, key)?
            }
            _ => quench_tls::client_config(&options)?,
        };
        let stream = quench_tls::TlsConnector::new(tls_config)
            .connect(quench_tls::server_name(&host)?, tcp)
            .await
            .map_err(|e| format!("TLS to the API server: {e}"))?;
        run(stream, &request).await
    } else {
        run(tcp, &request).await
    }
}

async fn run<I: quench_proto::io::Io>(mut io: I, request: &str) -> Result<ExecConnection, String> {
    io.write_all(request.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    io.flush().await.map_err(|e| e.to_string())?;
    let head = read_head(&mut io).await?;
    let status = head.lines().next().unwrap_or_default();
    if !status.contains(" 101 ") {
        return Err(format!("the API server refused the shell: {status}"));
    }

    let mut socket = ws::client(io, ws::Config::default());
    let (input, mut from_person) = mpsc::channel::<Vec<u8>>(64);
    let (to_person, output) = mpsc::channel::<ExecEvent>(256);
    tokio::spawn(async move {
        loop {
            tokio::select! {
                typed = from_person.recv() => {
                    // The person closed the session.
                    let Some(bytes) = typed else {
                        let _ = socket.close(1000, "done").await;
                        return;
                    };
                    let mut frame = vec![CHANNEL_STDIN];
                    frame.extend(bytes);
                    if socket.send(Message::Binary(frame)).await.is_err() {
                        let _ = to_person.send(ExecEvent::Closed(Some("the connection broke".into()))).await;
                        return;
                    }
                }
                frame = socket.recv() => match frame {
                    Ok(Some(Message::Binary(data))) => {
                        if let Some(event) = event_of(&data) {
                            let ended = matches!(event, ExecEvent::Closed(_));
                            if to_person.send(event).await.is_err() || ended {
                                return;
                            }
                        }
                    }
                    Ok(Some(_)) => {}
                    Ok(None) => {
                        let _ = to_person.send(ExecEvent::Closed(None)).await;
                        return;
                    }
                    Err(error) => {
                        let _ = to_person.send(ExecEvent::Closed(Some(error.to_string()))).await;
                        return;
                    }
                },
            }
        }
    });
    Ok(ExecConnection { input, output })
}

// ---------------------------------------------------------------- the person's end

struct Buffer {
    bytes: Vec<u8>,
    /// How many bytes were dropped off the front: offsets are absolute, so a reader can say where it was.
    dropped: u64,
    ended: bool,
    reason: Option<String>,
}

struct Session {
    owner: String,
    namespace: String,
    pod: String,
    input: mpsc::Sender<Vec<u8>>,
    buffer: Arc<Mutex<Buffer>>,
    last_used: Mutex<Instant>,
}

/// What a read of a session gives back.
#[derive(Debug, Clone, PartialEq, Eq, quench_serde::Serialize)]
pub struct Read {
    pub data: String,
    /// Where to ask from next.
    pub next: u64,
    /// The session is over (the reason, when there is one).
    pub closed: Option<String>,
    pub ended: bool,
}

/// Every open shell.
#[derive(Default)]
pub struct ExecSessions {
    sessions: Mutex<BTreeMap<String, Arc<Session>>>,
}

#[derive(Debug, quench_core::Error)]
pub enum ExecError {
    #[error("{0}")]
    Refused(String),
    #[error("there is no such session (it may have timed out)")]
    Unknown,
}

impl ExecSessions {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn sweep(&self) {
        lock(&self.sessions).retain(|_, s| lock(&s.last_used).elapsed() < IDLE);
    }

    /// Starts keeping what `connection` prints. The session's id is what the page then uses.
    ///
    /// # Errors
    ///
    /// [`ExecError::Refused`] when too many shells are open.
    pub fn start(
        &self,
        owner: &str,
        namespace: &str,
        pod: &str,
        connection: ExecConnection,
    ) -> Result<String, ExecError> {
        self.sweep();
        let mut sessions = lock(&self.sessions);
        if sessions.len() >= MAX_SESSIONS {
            return Err(ExecError::Refused(format!(
                "{MAX_SESSIONS} shells are open already; close one first"
            )));
        }
        let id = quench_core::random::bytes::<8>()
            .iter()
            .fold(String::new(), |mut id, b| {
                let _ = write!(id, "{b:02x}");
                id
            });
        let buffer = Arc::new(Mutex::new(Buffer {
            bytes: Vec::new(),
            dropped: 0,
            ended: false,
            reason: None,
        }));
        let (kept, mut output) = (Arc::clone(&buffer), connection.output);
        tokio::spawn(async move {
            while let Some(event) = output.recv().await {
                let finished = {
                    let mut buffer = lock(&kept);
                    match event {
                        ExecEvent::Output(data) => {
                            buffer.bytes.extend(data);
                            if buffer.bytes.len() > BUFFER_BYTES {
                                let over = buffer.bytes.len() - BUFFER_BYTES;
                                buffer.bytes.drain(..over);
                                buffer.dropped += over as u64;
                            }
                            false
                        }
                        ExecEvent::Closed(why) => {
                            buffer.ended = true;
                            buffer.reason = why;
                            true
                        }
                    }
                };
                if finished {
                    return;
                }
            }
            lock(&kept).ended = true;
        });
        sessions.insert(
            id.clone(),
            Arc::new(Session {
                owner: owner.to_string(),
                namespace: namespace.to_string(),
                pod: pod.to_string(),
                input: connection.input,
                buffer,
                last_used: Mutex::new(Instant::now()),
            }),
        );
        drop(sessions);
        Ok(id)
    }

    fn get(&self, id: &str, caller: &str) -> Result<Arc<Session>, ExecError> {
        self.sweep();
        let session = lock(&self.sessions)
            .get(id)
            .cloned()
            .ok_or(ExecError::Unknown)?;
        // A shell belongs to the person who opened it.
        if session.owner != caller {
            return Err(ExecError::Unknown);
        }
        *lock(&session.last_used) = Instant::now();
        Ok(session)
    }

    /// Types into the shell.
    ///
    /// # Errors
    ///
    /// [`ExecError::Unknown`] for a session that is gone or not the caller's; [`ExecError::Refused`] once it
    /// has ended.
    pub async fn write(&self, id: &str, caller: &str, text: &str) -> Result<(), ExecError> {
        let session = self.get(id, caller)?;
        session
            .input
            .send(text.as_bytes().to_vec())
            .await
            .map_err(|_| ExecError::Refused("the shell has ended".into()))
    }

    /// What the shell printed since `since`.
    ///
    /// # Errors
    ///
    /// [`ExecError::Unknown`].
    pub fn read(&self, id: &str, caller: &str, since: u64) -> Result<Read, ExecError> {
        let session = self.get(id, caller)?;
        let buffer = lock(&session.buffer);
        // Output dropped before the reader got to it is gone: resume where the buffer starts.
        let from = usize::try_from(since.max(buffer.dropped) - buffer.dropped).unwrap_or(0);
        let tail = &buffer.bytes[from.min(buffer.bytes.len())..];
        // A character cut in two by the end of the buffer waits for its other half.
        let whole = match std::str::from_utf8(tail) {
            Err(error) if error.error_len().is_none() => error.valid_up_to(),
            _ => tail.len(),
        };
        let data = String::from_utf8_lossy(&tail[..whole]).into_owned();
        Ok(Read {
            data,
            next: buffer.dropped + (from + whole) as u64,
            ended: buffer.ended,
            closed: buffer.reason.clone(),
        })
    }

    /// Ends the session.
    pub fn close(&self, id: &str, caller: &str) {
        if self.get(id, caller).is_ok() {
            lock(&self.sessions).remove(id);
        }
    }

    /// Which pod each open shell is in, for the log.
    #[must_use]
    pub fn open(&self) -> Vec<(String, String)> {
        lock(&self.sessions)
            .values()
            .map(|s| (s.namespace.clone(), s.pod.clone()))
            .collect()
    }
}

impl std::fmt::Debug for ExecSessions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ExecSessions")
    }
}

/// A shell that answers each line it is given: for a fake cluster and for tests.
#[must_use]
pub fn echo_shell() -> ExecConnection {
    let (input, mut typed) = mpsc::channel::<Vec<u8>>(64);
    let (to_person, output) = mpsc::channel::<ExecEvent>(64);
    tokio::spawn(async move {
        let _ = to_person.send(ExecEvent::Output(b"$ ".to_vec())).await;
        while let Some(line) = typed.recv().await {
            let text = String::from_utf8_lossy(&line).trim().to_string();
            if text == "exit" {
                let _ = to_person.send(ExecEvent::Closed(None)).await;
                return;
            }
            let _ = to_person
                .send(ExecEvent::Output(format!("{text}\r\n$ ").into_bytes()))
                .await;
        }
    });
    ExecConnection { input, output }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_server_address_splits_into_host_port_and_scheme() {
        assert_eq!(
            endpoint("https://10.0.0.1:6443").unwrap(),
            ("10.0.0.1".into(), 6443, true)
        );
        assert_eq!(
            endpoint("https://kube.local").unwrap(),
            ("kube.local".into(), 443, true)
        );
        assert_eq!(
            endpoint("http://[::1]:8001").unwrap(),
            ("[::1]".into(), 8001, false)
        );
        assert!(endpoint("kube.local").is_err());
    }
}
