//! An IMAP4rev1 client: what a mail reader needs and nothing more.
//!
//! Connect (TLS or STARTTLS, never credentials in the clear), log in (PLAIN, LOGIN or XOAUTH2), list folders, select one,
//! fetch headers and bodies by UID, search, change flags, move and copy, append. Responses are read with a small tokenizer
//! with hard limits (line length, literal size, nesting) because the server is somebody else's. Every user-supplied string
//! reaches the wire only through [`Command::quoted`] or [`Command::mailbox`], which quote it or send it as a literal.

use crate::netguard::{self, Policy, Refusal};
use crate::presets::Security;
use quench_tls::{ClientOptions, TlsConnector, TlsStream};
use std::fmt;
use std::fmt::Write as _;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::TcpStream;
use tokio::time::timeout;

/// Longest response line (not counting literals) accepted.
const MAX_LINE: usize = 256 * 1024;
/// Deepest parenthesis nesting read.
const MAX_DEPTH: usize = 32;
/// Default cap for the bytes of one response, literals included.
const DEFAULT_MAX_RESPONSE: usize = 64 * 1024 * 1024;
/// Longest a single command may take in all, and most untagged responses it may collect.
const COMMAND_DEADLINE: Duration = Duration::from_secs(300);
const MAX_UNTAGGED: usize = 200_000;

/// What can go wrong.
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    /// The TLS handshake or setup failed (an untrusted certificate, say).
    Tls(String),
    /// A read or write took longer than the timeout.
    Timeout,
    /// The server said something that is not IMAP, or too much of it.
    Protocol(String),
    /// The destination is not allowed (see [`netguard`]).
    Refused(Refusal),
    /// The server answered `NO`.
    No(String),
    /// The server answered `BAD`.
    Bad(String),
    /// The login was refused (wrong password, IMAP switched off, app password needed).
    Auth(String),
    /// The server hung up (`BYE`).
    Bye(String),
    /// The server will not offer a secure connection and we will not send a password without one.
    Insecure,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "connection error: {e}"),
            Self::Tls(e) => write!(f, "TLS error: {e}"),
            Self::Timeout => f.write_str("the server took too long to answer"),
            Self::Protocol(e) => write!(f, "protocol error: {e}"),
            Self::Refused(r) => write!(f, "{r}"),
            Self::No(t) => write!(f, "the server refused: {t}"),
            Self::Bad(t) => write!(f, "the server did not understand: {t}"),
            Self::Auth(t) => write!(f, "login refused: {t}"),
            Self::Bye(t) => write!(f, "the server closed the connection: {t}"),
            Self::Insecure => f.write_str("the server offers no secure connection"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        if e.kind() == io::ErrorKind::PermissionDenied
            && let Some(inner) = e.get_ref()
            && let Some(refusal) = inner.downcast_ref::<Refusal>()
        {
            return Self::Refused(refusal.clone());
        }
        Self::Io(e)
    }
}

/// How the client proves who it is.
#[derive(Clone)]
pub enum Auth {
    /// A password or an app password.
    Password(String),
    /// An OAuth 2 access token (SASL XOAUTH2).
    XOAuth2(String),
}

impl fmt::Debug for Auth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Password(_) => "Auth::Password(<redacted>)",
            Self::XOAuth2(_) => "Auth::XOAuth2(<redacted>)",
        })
    }
}

/// Everything needed to open a session.
#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub security: Security,
    pub username: String,
    pub auth: Auth,
    /// Where connections may go.
    pub policy: Policy,
    /// Per read and write; default 30 s.
    pub timeout: Duration,
    /// Extra trusted certificates (PEM), for a server with its own CA or a self-signed certificate.
    pub tls_roots_pem: Vec<String>,
    /// The name the certificate must match, when it is not `host`.
    pub tls_server_name: Option<String>,
    /// Most bytes of one response (literals included).
    pub max_response: usize,
}

impl Config {
    /// A config with the defaults for everything but the account.
    #[must_use]
    pub fn new(host: &str, port: u16, security: Security, username: &str, auth: Auth) -> Self {
        Self {
            host: host.to_string(),
            port,
            security,
            username: username.to_string(),
            auth,
            policy: Policy::default(),
            timeout: Duration::from_secs(30),
            tls_roots_pem: Vec::new(),
            tls_server_name: None,
            max_response: DEFAULT_MAX_RESPONSE,
        }
    }
}

enum Stream {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
    Closed,
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(s) => Pin::new(s).poll_read(cx, buf),
            Self::Tls(s) => Pin::new(&mut **s).poll_read(cx, buf),
            Self::Closed => Poll::Ready(Err(io::ErrorKind::NotConnected.into())),
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Self::Plain(s) => Pin::new(s).poll_write(cx, data),
            Self::Tls(s) => Pin::new(&mut **s).poll_write(cx, data),
            Self::Closed => Poll::Ready(Err(io::ErrorKind::NotConnected.into())),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(s) => Pin::new(s).poll_flush(cx),
            Self::Tls(s) => Pin::new(&mut **s).poll_flush(cx),
            Self::Closed => Poll::Ready(Ok(())),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(s) => Pin::new(s).poll_shutdown(cx),
            Self::Tls(s) => Pin::new(&mut **s).poll_shutdown(cx),
            Self::Closed => Poll::Ready(Ok(())),
        }
    }
}

/// A parsed response item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Nil,
    /// A bare word or number.
    Atom(String),
    /// A quoted string or a literal, as bytes.
    Str(Vec<u8>),
    List(Vec<Self>),
}

impl Value {
    /// The text of an atom or string (lossy for non-UTF-8 bytes).
    #[must_use]
    pub fn text(&self) -> Option<String> {
        match self {
            Self::Atom(a) => Some(a.clone()),
            Self::Str(s) => Some(String::from_utf8_lossy(s).into_owned()),
            _ => None,
        }
    }

    /// The bytes of a string, or the text of an atom.
    #[must_use]
    pub fn bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Str(s) => Some(s),
            Self::Atom(a) => Some(a.as_bytes()),
            _ => None,
        }
    }

    /// The number an atom holds.
    #[must_use]
    pub fn number(&self) -> Option<u64> {
        match self {
            Self::Atom(a) => a.parse().ok(),
            _ => None,
        }
    }

    /// The items of a list.
    #[must_use]
    pub fn list(&self) -> Option<&[Self]> {
        match self {
            Self::List(l) => Some(l),
            _ => None,
        }
    }
}

struct Lexer<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Lexer<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn skip_space(&mut self) {
        while self
            .bytes
            .get(self.at)
            .is_some_and(|b| *b == b' ' || *b == b'\t')
        {
            self.at += 1;
        }
    }

    /// The next value; `Ok(None)` at a closing parenthesis (not consumed) or the end.
    fn value(&mut self, depth: usize) -> Result<Option<Value>, Error> {
        self.skip_space();
        let Some(&byte) = self.bytes.get(self.at) else {
            return Ok(None);
        };
        match byte {
            b')' | b'\r' | b'\n' => Ok(None),
            b'(' => {
                if depth >= MAX_DEPTH {
                    return Err(Error::Protocol("response nested too deeply".into()));
                }
                self.at += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_space();
                    match self.bytes.get(self.at) {
                        Some(b')') => {
                            self.at += 1;
                            break;
                        }
                        None => return Err(Error::Protocol("unterminated list".into())),
                        _ => match self.value(depth + 1)? {
                            Some(v) => items.push(v),
                            None => return Err(Error::Protocol("unterminated list".into())),
                        },
                    }
                }
                Ok(Some(Value::List(items)))
            }
            b'"' => {
                self.at += 1;
                let mut out = Vec::new();
                loop {
                    match self.bytes.get(self.at) {
                        None => return Err(Error::Protocol("unterminated string".into())),
                        Some(b'"') => {
                            self.at += 1;
                            break;
                        }
                        Some(b'\\') => {
                            self.at += 1;
                            let Some(&escaped) = self.bytes.get(self.at) else {
                                return Err(Error::Protocol("unterminated string".into()));
                            };
                            out.push(escaped);
                            self.at += 1;
                        }
                        Some(&b) => {
                            out.push(b);
                            self.at += 1;
                        }
                    }
                }
                Ok(Some(Value::Str(out)))
            }
            b'{' => {
                let close = self.bytes[self.at..]
                    .iter()
                    .position(|b| *b == b'}')
                    .ok_or_else(|| Error::Protocol("bad literal marker".into()))?;
                let marker = std::str::from_utf8(&self.bytes[self.at + 1..self.at + close])
                    .map_err(|_| Error::Protocol("bad literal marker".into()))?;
                let length: usize = marker
                    .trim_end_matches('+')
                    .parse()
                    .map_err(|_| Error::Protocol("bad literal length".into()))?;
                self.at += close + 1;
                // CRLF after the marker.
                if self.bytes.get(self.at) == Some(&b'\r') {
                    self.at += 1;
                }
                if self.bytes.get(self.at) == Some(&b'\n') {
                    self.at += 1;
                }
                let end = self
                    .at
                    .checked_add(length)
                    .filter(|end| *end <= self.bytes.len())
                    .ok_or_else(|| Error::Protocol("literal longer than the response".into()))?;
                let data = self.bytes[self.at..end].to_vec();
                self.at = end;
                Ok(Some(Value::Str(data)))
            }
            _ => {
                let start = self.at;
                let mut bracket = 0usize;
                while let Some(&b) = self.bytes.get(self.at) {
                    match b {
                        b'[' => bracket += 1,
                        b']' => bracket = bracket.saturating_sub(1),
                        b' ' | b'(' | b')' | b'\r' | b'\n' if bracket == 0 => break,
                        b'"' if bracket > 0 => {
                            // A quoted string inside a section specifier.
                            self.at += 1;
                            while self.bytes.get(self.at).is_some_and(|c| *c != b'"') {
                                self.at += 1;
                            }
                        }
                        _ => {}
                    }
                    self.at += 1;
                }
                let atom =
                    String::from_utf8_lossy(&self.bytes[start..self.at.min(self.bytes.len())])
                        .into_owned();
                if atom.eq_ignore_ascii_case("NIL") {
                    Ok(Some(Value::Nil))
                } else {
                    Ok(Some(Value::Atom(atom)))
                }
            }
        }
    }

    /// Every remaining value on the line.
    fn rest(&mut self) -> Result<Vec<Value>, Error> {
        let mut out = Vec::new();
        while let Some(v) = self.value(0)? {
            out.push(v);
        }
        Ok(out)
    }
}

/// How a command ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Ok,
    No,
    Bad,
}

/// What the server sent that was not the command's own result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Untagged {
    Exists(u32),
    Expunge(u32),
    Recent(u32),
    Fetch {
        seq: u32,
        items: Vec<Value>,
    },
    List(Folder),
    Search(Vec<u32>),
    Status {
        mailbox: String,
        items: Vec<(String, u64)>,
    },
    Capability(Vec<String>),
    /// `OK`/`NO`/`BAD` with a bracketed code (`UIDVALIDITY`, `UIDNEXT`, ...) and the text.
    Condition {
        status: Status,
        code: Option<(String, String)>,
        text: String,
    },
    Bye(String),
    Flags(Vec<String>),
    Other(String),
}

/// A command's outcome and everything the server said meanwhile.
#[derive(Debug, Clone)]
pub struct Completed {
    pub status: Status,
    /// The code in the tagged response (`[APPENDUID 1 2]`, `[CAPABILITY ...]`), if any.
    pub code: Option<(String, String)>,
    pub text: String,
    pub untagged: Vec<Untagged>,
}

/// A folder as `LIST` describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    /// The name as the server has it (modified UTF-7 decoded to Unicode).
    pub path: String,
    pub delimiter: Option<char>,
    /// Attributes, lower-case, without the backslash (`noselect`, `sent`, ...).
    pub attributes: Vec<String>,
}

impl Folder {
    /// Whether messages can be in it.
    #[must_use]
    pub fn selectable(&self) -> bool {
        !self
            .attributes
            .iter()
            .any(|a| a == "noselect" || a == "nonexistent")
    }

    /// The folder's role: `inbox`, `sent`, `drafts`, `trash`, `junk`, `archive`, `all`, `flagged`, or empty.
    #[must_use]
    pub fn role(&self) -> &'static str {
        if self.path.eq_ignore_ascii_case("INBOX") {
            return "inbox";
        }
        for (attribute, role) in [
            ("sent", "sent"),
            ("drafts", "drafts"),
            ("trash", "trash"),
            ("junk", "junk"),
            ("archive", "archive"),
            ("all", "all"),
            ("flagged", "flagged"),
        ] {
            if self.attributes.iter().any(|a| a == attribute) {
                return role;
            }
        }
        // Servers without SPECIAL-USE: go by the usual names.
        let name = self
            .path
            .rsplit(self.delimiter.unwrap_or('/'))
            .next()
            .unwrap_or(&self.path);
        match name.to_lowercase().as_str() {
            "sent" | "sent items" | "sent mail" | "sent messages" | "wysłane"
            | "elementy wysłane" => "sent",
            "drafts" | "draft" | "wersje robocze" | "szkice" => "drafts",
            "trash" | "deleted items" | "deleted messages" | "bin" | "kosz"
            | "elementy usunięte" => "trash",
            "junk" | "spam" | "junk e-mail" | "junk email" => "junk",
            "archive" | "archives" | "archiwum" => "archive",
            _ => "",
        }
    }
}

/// The state of a selected folder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selected {
    pub exists: u32,
    pub uid_validity: u32,
    pub uid_next: u32,
    pub highest_modseq: Option<u64>,
    pub read_only: bool,
}

/// One message as the list sync fetches it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    pub uid: u32,
    /// Flags, as sent (`\Seen`, `$Forwarded`, ...).
    pub flags: Vec<String>,
    /// `INTERNALDATE` as Unix seconds.
    pub internal_date: Option<i64>,
    pub size: u32,
    /// The header bytes asked for (or the whole message when the fetch was of the body).
    pub data: Vec<u8>,
}

/// A command under construction: raw parts and literals.
#[derive(Debug, Default)]
pub struct Command {
    parts: Vec<Part>,
}

#[derive(Debug)]
enum Part {
    Raw(String),
    Literal(Vec<u8>),
}

impl Command {
    /// Starts a command with its name (and any arguments that are known-safe words).
    #[must_use]
    pub fn new(words: &str) -> Self {
        Self {
            parts: vec![Part::Raw(words.to_string())],
        }
    }

    /// Appends known-safe text (keywords, numbers, UID sets). Never pass user input here.
    #[must_use]
    pub fn raw(mut self, text: &str) -> Self {
        self.parts.push(Part::Raw(text.to_string()));
        self
    }

    /// Appends a user string, quoted when plain and sent as a literal when it holds characters a quoted string cannot.
    #[must_use]
    pub fn quoted(mut self, text: &str) -> Self {
        if text
            .bytes()
            .any(|b| b == 0 || b == b'\r' || b == b'\n' || b >= 0x80)
        {
            self.parts.push(Part::Literal(text.as_bytes().to_vec()));
        } else {
            let mut quoted = String::with_capacity(text.len() + 2);
            quoted.push('"');
            for c in text.chars() {
                if c == '"' || c == '\\' {
                    quoted.push('\\');
                }
                quoted.push(c);
            }
            quoted.push('"');
            self.parts.push(Part::Raw(quoted));
        }
        self
    }

    /// Appends a mailbox name: modified UTF-7, quoted.
    #[must_use]
    pub fn mailbox(self, path: &str) -> Self {
        let encoded = utf7_encode(path);
        // Modified UTF-7 is pure ASCII, but may hold a quote or backslash in a literal name.
        self.quoted(&encoded)
    }

    /// Appends a message literal (for `APPEND`).
    #[must_use]
    pub fn literal(mut self, data: Vec<u8>) -> Self {
        self.parts.push(Part::Literal(data));
        self
    }
}

/// A logged-in session.
pub struct Client {
    stream: Stream,
    buffer: Vec<u8>,
    tag: u32,
    timeout: Duration,
    max_response: usize,
    capabilities: Vec<String>,
    secure: bool,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client")
            .field("secure", &self.secure)
            .field("capabilities", &self.capabilities)
            .finish_non_exhaustive()
    }
}

impl Client {
    /// Connects, secures the connection, and logs in.
    ///
    /// # Errors
    ///
    /// See [`Error`]: a refused destination, a connection or TLS failure, or a refused login.
    pub async fn connect(config: &Config) -> Result<Self, Error> {
        let addresses = netguard::resolve(&config.host, config.port, &config.policy)
            .await
            .map_err(Error::Refused)?;
        // Connect to addresses that were checked, not to the name; the next one when an address does not answer.
        let mut last = Error::Io(io::ErrorKind::NotFound.into());
        let mut connected = None;
        for address in &addresses {
            match tokio::time::timeout(
                config
                    .policy
                    .connect_timeout
                    .unwrap_or(Duration::from_secs(15)),
                TcpStream::connect(address),
            )
            .await
            {
                Ok(Ok(tcp)) => {
                    connected = Some(tcp);
                    break;
                }
                Ok(Err(e)) => last = Error::Io(e),
                Err(_) => last = Error::Timeout,
            }
        }
        let tcp = connected.ok_or(last)?;
        let _ = tcp.set_nodelay(true);

        let connector = connector(config)?;
        let name = config
            .tls_server_name
            .clone()
            .unwrap_or_else(|| config.host.clone());
        let server_name = quench_tls::server_name(&name).map_err(Error::Tls)?;

        let (stream, secure) = match config.security {
            Security::Tls => {
                let tls = timeout(config.timeout, connector.connect(server_name.clone(), tcp))
                    .await
                    .map_err(|_| Error::Timeout)?
                    .map_err(|e| Error::Tls(e.to_string()))?;
                (Stream::Tls(Box::new(tls)), true)
            }
            Security::StartTls => (Stream::Plain(tcp), false),
        };
        let mut client = Self {
            stream,
            buffer: Vec::new(),
            tag: 0,
            timeout: config.timeout,
            max_response: config.max_response,
            capabilities: Vec::new(),
            secure,
        };

        // Greeting.
        let greeting = client.read_response().await?;
        match parse_untagged(&greeting)? {
            Untagged::Bye(text) => return Err(Error::Bye(text)),
            Untagged::Condition {
                code: Some((name, value)),
                ..
            } if name == "CAPABILITY" => {
                client.capabilities = words(&value);
            }
            _ => {}
        }
        if client.capabilities.is_empty() {
            client.refresh_capabilities().await?;
        }

        if config.security == Security::StartTls {
            if !client.has("STARTTLS") {
                return Err(Error::Insecure);
            }
            client.run(Command::new("STARTTLS")).await?.require_ok()?;
            // Whatever arrived before the handshake was sent in the clear: it cannot be trusted afterwards.
            if !client.buffer.is_empty() {
                return Err(Error::Protocol("data before the TLS handshake".into()));
            }
            let Stream::Plain(tcp) = std::mem::replace(&mut client.stream, Stream::Closed) else {
                return Err(Error::Protocol("cannot upgrade this connection".into()));
            };
            let tls = timeout(config.timeout, connector.connect(server_name, tcp))
                .await
                .map_err(|_| Error::Timeout)?
                .map_err(|e| Error::Tls(e.to_string()))?;
            client.stream = Stream::Tls(Box::new(tls));
            client.secure = true;
            client.capabilities.clear();
            client.refresh_capabilities().await?;
        }

        client.login(config).await?;
        Ok(client)
    }

    fn has(&self, capability: &str) -> bool {
        self.capabilities
            .iter()
            .any(|c| c.eq_ignore_ascii_case(capability))
    }

    /// The capabilities the server announced.
    #[must_use]
    pub fn capabilities(&self) -> &[String] {
        &self.capabilities
    }

    async fn refresh_capabilities(&mut self) -> Result<(), Error> {
        let done = self.run(Command::new("CAPABILITY")).await?;
        for item in &done.untagged {
            if let Untagged::Capability(list) = item {
                self.capabilities.clone_from(list);
            }
        }
        Ok(())
    }

    async fn login(&mut self, config: &Config) -> Result<(), Error> {
        if !self.secure {
            return Err(Error::Insecure);
        }
        let done = match &config.auth {
            Auth::Password(password) => {
                if self.has("AUTH=PLAIN") {
                    let token = crate::encoding::base64_encode(
                        format!("\0{}\0{password}", config.username).as_bytes(),
                    );
                    self.authenticate("PLAIN", &token).await
                } else if self.has("LOGINDISABLED") {
                    return Err(Error::Auth(
                        "the server does not allow password login".into(),
                    ));
                } else {
                    self.run(
                        Command::new("LOGIN")
                            .raw(" ")
                            .quoted(&config.username)
                            .raw(" ")
                            .quoted(password),
                    )
                    .await
                }
            }
            Auth::XOAuth2(token) => {
                if !self.has("AUTH=XOAUTH2") {
                    return Err(Error::Auth("the server does not offer XOAUTH2".into()));
                }
                let initial = crate::encoding::base64_encode(
                    format!("user={}\x01auth=Bearer {token}\x01\x01", config.username).as_bytes(),
                );
                self.authenticate("XOAUTH2", &initial).await
            }
        };
        match done {
            Ok(done) if done.status == Status::Ok => {
                if let Some((name, value)) = &done.code
                    && name == "CAPABILITY"
                {
                    self.capabilities = words(value);
                } else {
                    self.refresh_capabilities().await?;
                }
                Ok(())
            }
            Ok(done) => Err(Error::Auth(done.text)),
            Err(Error::No(text) | Error::Bad(text)) => Err(Error::Auth(text)),
            Err(e) => Err(e),
        }
    }

    async fn authenticate(&mut self, mechanism: &str, initial: &str) -> Result<Completed, Error> {
        self.tag += 1;
        let tag = format!("a{}", self.tag);
        if self.has("SASL-IR") {
            self.write(format!("{tag} AUTHENTICATE {mechanism} {initial}\r\n").as_bytes())
                .await?;
        } else {
            self.write(format!("{tag} AUTHENTICATE {mechanism}\r\n").as_bytes())
                .await?;
            let line = self.read_response().await?;
            if !line.starts_with(b"+") {
                return Err(Error::Protocol("expected a continuation".into()));
            }
            self.write(format!("{initial}\r\n").as_bytes()).await?;
        }
        let mut untagged = Vec::new();
        loop {
            let response = self.read_response().await?;
            if response.starts_with(b"+") {
                // XOAUTH2 error details: answer with an empty line to get the final NO.
                self.write(b"\r\n").await?;
                continue;
            }
            if let Some(done) = parse_tagged(&response, &tag)? {
                return Ok(Completed { untagged, ..done });
            }
            untagged.push(parse_untagged(&response)?);
            if untagged.len() > MAX_UNTAGGED {
                return Err(Error::Protocol("too many responses".into()));
            }
        }
    }

    async fn write(&mut self, data: &[u8]) -> Result<(), Error> {
        timeout(self.timeout, async {
            self.stream.write_all(data).await?;
            self.stream.flush().await
        })
        .await
        .map_err(|_| Error::Timeout)??;
        Ok(())
    }

    /// Reads one whole response: a line, and the literals it announces, with the line's remainder after each.
    async fn read_response(&mut self) -> Result<Vec<u8>, Error> {
        let mut response: Vec<u8> = Vec::new();
        loop {
            let line = self.read_line().await?;
            response.extend_from_slice(&line);
            // Does the line end with a literal marker?
            let trimmed = line
                .strip_suffix(b"\r\n")
                .or_else(|| line.strip_suffix(b"\n"))
                .unwrap_or(&line);
            let Some(length) = literal_length(trimmed) else {
                return Ok(response);
            };
            if response.len().saturating_add(length) > self.max_response {
                return Err(Error::Protocol("response too large".into()));
            }
            let data = self.read_exact(length).await?;
            response.extend_from_slice(&data);
        }
    }

    async fn read_line(&mut self) -> Result<Vec<u8>, Error> {
        loop {
            if let Some(newline) = self.buffer.iter().position(|b| *b == b'\n') {
                let rest = self.buffer.split_off(newline + 1);
                return Ok(std::mem::replace(&mut self.buffer, rest));
            }
            if self.buffer.len() > MAX_LINE {
                return Err(Error::Protocol("line too long".into()));
            }
            self.fill().await?;
        }
    }

    async fn read_exact(&mut self, length: usize) -> Result<Vec<u8>, Error> {
        while self.buffer.len() < length {
            self.fill().await?;
        }
        let rest = self.buffer.split_off(length);
        Ok(std::mem::replace(&mut self.buffer, rest))
    }

    async fn fill(&mut self) -> Result<(), Error> {
        let mut chunk = vec![0u8; 16 * 1024];
        let n = timeout(self.timeout, self.stream.read(&mut chunk))
            .await
            .map_err(|_| Error::Timeout)??;
        if n == 0 {
            return Err(Error::Io(io::ErrorKind::UnexpectedEof.into()));
        }
        self.buffer.extend_from_slice(&chunk[..n]);
        Ok(())
    }

    /// Sends a command and reads until its tagged response.
    ///
    /// # Errors
    ///
    /// Connection and protocol failures; a `NO` or `BAD` is returned as `Ok` with the status (use
    /// [`Completed::require_ok`]), except while logging in.
    pub async fn run(&mut self, command: Command) -> Result<Completed, Error> {
        // A server may be slow (each read has its own timeout) but not endless: the whole command has a deadline too.
        timeout(COMMAND_DEADLINE, self.run_inner(command))
            .await
            .map_err(|_| Error::Timeout)?
    }

    async fn run_inner(&mut self, command: Command) -> Result<Completed, Error> {
        self.tag += 1;
        let tag = format!("a{}", self.tag);
        let mut line = format!("{tag} ");
        let mut untagged = Vec::new();
        for part in command.parts {
            match part {
                Part::Raw(text) => line.push_str(&text),
                Part::Literal(data) => {
                    let _ = write!(line, "{{{}}}\r\n", data.len());
                    self.write(line.as_bytes()).await?;
                    line.clear();
                    // Wait for "+ go ahead", taking whatever else arrives meanwhile.
                    loop {
                        let response = self.read_response().await?;
                        if response.starts_with(b"+") {
                            break;
                        }
                        if let Some(done) = parse_tagged(&response, &tag)? {
                            // The server refused the literal.
                            return Ok(Completed { untagged, ..done });
                        }
                        untagged.push(parse_untagged(&response)?);
                        if untagged.len() > MAX_UNTAGGED {
                            return Err(Error::Protocol("too many responses".into()));
                        }
                    }
                    self.write(&data).await?;
                }
            }
        }
        line.push_str("\r\n");
        self.write(line.as_bytes()).await?;
        loop {
            let response = self.read_response().await?;
            if response.starts_with(b"+") {
                return Err(Error::Protocol("unexpected continuation".into()));
            }
            if let Some(done) = parse_tagged(&response, &tag)? {
                for item in &untagged {
                    if let Untagged::Bye(text) = item
                        && done.status != Status::Ok
                    {
                        return Err(Error::Bye(text.clone()));
                    }
                }
                return Ok(Completed { untagged, ..done });
            }
            untagged.push(parse_untagged(&response)?);
            if untagged.len() > MAX_UNTAGGED {
                return Err(Error::Protocol("too many responses".into()));
            }
        }
    }

    /// `LIST "" "*"`: every folder, with roles resolved.
    ///
    /// # Errors
    ///
    /// Any [`Error`].
    pub async fn list_folders(&mut self) -> Result<Vec<Folder>, Error> {
        let done = self
            .run(Command::new("LIST ").quoted("").raw(" ").quoted("*"))
            .await?
            .require_ok()?;
        Ok(done
            .untagged
            .into_iter()
            .filter_map(|u| match u {
                Untagged::List(f) => Some(f),
                _ => None,
            })
            .collect())
    }

    /// `SELECT` (or `EXAMINE` when `read_only`).
    ///
    /// # Errors
    ///
    /// Any [`Error`]; a missing folder is [`Error::No`].
    pub async fn select(&mut self, path: &str, read_only: bool) -> Result<Selected, Error> {
        let verb = if read_only { "EXAMINE " } else { "SELECT " };
        let done = self
            .run(Command::new(verb).mailbox(path))
            .await?
            .require_ok()?;
        let mut selected = Selected {
            read_only: read_only || done.code.as_ref().is_some_and(|(n, _)| n == "READ-ONLY"),
            ..Selected::default()
        };
        for item in &done.untagged {
            match item {
                Untagged::Exists(n) => selected.exists = *n,
                Untagged::Condition {
                    code: Some((name, value)),
                    ..
                } => match name.as_str() {
                    "UIDVALIDITY" => selected.uid_validity = value.trim().parse().unwrap_or(0),
                    "UIDNEXT" => selected.uid_next = value.trim().parse().unwrap_or(0),
                    "HIGHESTMODSEQ" => selected.highest_modseq = value.trim().parse().ok(),
                    _ => {}
                },
                _ => {}
            }
        }
        Ok(selected)
    }

    /// `UID SEARCH` with ready-made criteria (built from [`Command`]-safe pieces by the caller; see [`Search`]).
    ///
    /// # Errors
    ///
    /// Any [`Error`].
    pub async fn uid_search(&mut self, search: Search) -> Result<Vec<u32>, Error> {
        let done = self.run(search.0).await?.require_ok()?;
        let mut uids: Vec<u32> = done
            .untagged
            .into_iter()
            .filter_map(|u| match u {
                Untagged::Search(ids) => Some(ids),
                _ => None,
            })
            .flatten()
            .collect();
        uids.sort_unstable();
        uids.dedup();
        Ok(uids)
    }

    /// Fetches the headers named in `fields`, flags, date and size of the messages in `uid_set` (`1:10,15`).
    ///
    /// # Errors
    ///
    /// Any [`Error`].
    pub async fn fetch_headers(
        &mut self,
        uid_set: &str,
        fields: &str,
    ) -> Result<Vec<Fetched>, Error> {
        check_set(uid_set)?;
        let command = Command::new(&format!(
            "UID FETCH {uid_set} (UID FLAGS INTERNALDATE RFC822.SIZE BODY.PEEK[HEADER.FIELDS ({})])",
            safe_words(fields)
        ));
        let done = self.run(command).await?.require_ok()?;
        Ok(collect_fetched(&done.untagged))
    }

    /// Fetches the current flags of the messages in `uid_set`.
    ///
    /// # Errors
    ///
    /// Any [`Error`].
    pub async fn fetch_flags(&mut self, uid_set: &str) -> Result<Vec<(u32, Vec<String>)>, Error> {
        check_set(uid_set)?;
        let done = self
            .run(Command::new(&format!("UID FETCH {uid_set} (UID FLAGS)")))
            .await?
            .require_ok()?;
        Ok(collect_fetched(&done.untagged)
            .into_iter()
            .map(|f| (f.uid, f.flags))
            .collect())
    }

    /// Fetches the whole message `uid` without marking it read.
    ///
    /// # Errors
    ///
    /// Any [`Error`]; [`Error::No`] when the message is gone.
    pub async fn fetch_message(&mut self, uid: u32) -> Result<Option<Vec<u8>>, Error> {
        let done = self
            .run(Command::new(&format!("UID FETCH {uid} (UID BODY.PEEK[])")))
            .await?
            .require_ok()?;
        Ok(collect_fetched(&done.untagged)
            .into_iter()
            .find(|f| f.uid == uid)
            .map(|f| f.data))
    }

    /// Adds, removes or replaces flags (`mode`: `+`, `-` or empty) on the messages in `uid_set`.
    ///
    /// # Errors
    ///
    /// Any [`Error`].
    pub async fn store_flags(
        &mut self,
        uid_set: &str,
        mode: &str,
        flags: &[&str],
    ) -> Result<(), Error> {
        check_set(uid_set)?;
        if !matches!(mode, "+" | "-" | "") {
            return Err(Error::Protocol("bad store mode".into()));
        }
        let list = flags
            .iter()
            .map(|f| check_flag(f))
            .collect::<Result<Vec<_>, _>>()?
            .join(" ");
        self.run(Command::new(&format!(
            "UID STORE {uid_set} {mode}FLAGS.SILENT ({list})"
        )))
        .await?
        .require_ok()?;
        Ok(())
    }

    /// Copies messages to another folder.
    ///
    /// # Errors
    ///
    /// Any [`Error`].
    pub async fn copy(&mut self, uid_set: &str, destination: &str) -> Result<(), Error> {
        check_set(uid_set)?;
        self.run(Command::new(&format!("UID COPY {uid_set} ")).mailbox(destination))
            .await?
            .require_ok()?;
        Ok(())
    }

    /// Moves messages (`MOVE` when the server has it, otherwise copy, flag deleted, expunge).
    ///
    /// # Errors
    ///
    /// Any [`Error`].
    pub async fn move_to(&mut self, uid_set: &str, destination: &str) -> Result<(), Error> {
        check_set(uid_set)?;
        if self.has("MOVE") {
            self.run(Command::new(&format!("UID MOVE {uid_set} ")).mailbox(destination))
                .await?
                .require_ok()?;
            return Ok(());
        }
        self.copy(uid_set, destination).await?;
        self.store_flags(uid_set, "+", &["\\Deleted"]).await?;
        self.expunge(Some(uid_set)).await
    }

    /// Permanently removes messages flagged `\Deleted` (only those in `uid_set` when the server has UIDPLUS).
    ///
    /// # Errors
    ///
    /// Any [`Error`].
    pub async fn expunge(&mut self, uid_set: Option<&str>) -> Result<(), Error> {
        let command = match uid_set {
            Some(set) if self.has("UIDPLUS") => {
                check_set(set)?;
                Command::new(&format!("UID EXPUNGE {set}"))
            }
            _ => Command::new("EXPUNGE"),
        };
        self.run(command).await?.require_ok()?;
        Ok(())
    }

    /// `APPEND`s a message to a folder (a copy in Sent, a draft), flagged as given. Returns the new UID when the server says.
    ///
    /// # Errors
    ///
    /// Any [`Error`].
    pub async fn append(
        &mut self,
        path: &str,
        flags: &[&str],
        message: Vec<u8>,
    ) -> Result<Option<u32>, Error> {
        let list = flags
            .iter()
            .map(|f| check_flag(f))
            .collect::<Result<Vec<_>, _>>()?
            .join(" ");
        let done = self
            .run(
                Command::new("APPEND ")
                    .mailbox(path)
                    .raw(&format!(" ({list}) "))
                    .literal(message),
            )
            .await?
            .require_ok()?;
        Ok(done.code.and_then(|(name, value)| {
            (name == "APPENDUID")
                .then(|| value.split_whitespace().nth(1).and_then(|u| u.parse().ok()))
                .flatten()
        }))
    }

    /// Creates a folder.
    ///
    /// # Errors
    ///
    /// Any [`Error`].
    pub async fn create_folder(&mut self, path: &str) -> Result<(), Error> {
        self.run(Command::new("CREATE ").mailbox(path))
            .await?
            .require_ok()?;
        Ok(())
    }

    /// Keeps the connection alive and picks up changes.
    ///
    /// # Errors
    ///
    /// Any [`Error`].
    pub async fn noop(&mut self) -> Result<(), Error> {
        self.run(Command::new("NOOP")).await?.require_ok()?;
        Ok(())
    }

    /// Says goodbye and closes. Errors are ignored: the session is over either way.
    pub async fn logout(mut self) {
        let _ = timeout(Duration::from_secs(3), self.run(Command::new("LOGOUT"))).await;
        let _ = self.stream.shutdown().await;
    }
}

/// A `UID SEARCH` under construction.
#[derive(Debug)]
pub struct Search(Command);

impl Search {
    /// All messages.
    #[must_use]
    pub fn all() -> Self {
        Self(Command::new("UID SEARCH ALL"))
    }

    /// Messages with a UID of at least `uid` (the new ones since a sync).
    #[must_use]
    pub fn uid_from(uid: u32) -> Self {
        Self(Command::new(&format!("UID SEARCH UID {uid}:*")))
    }

    /// Unread messages.
    #[must_use]
    pub fn unseen() -> Self {
        Self(Command::new("UID SEARCH UNSEEN"))
    }

    /// Messages where the subject, a sender or recipient, or the body contains `text`.
    #[must_use]
    pub fn text(text: &str) -> Self {
        Self(
            Command::new("UID SEARCH CHARSET UTF-8 OR OR SUBJECT ")
                .quoted(text)
                .raw(" FROM ")
                .quoted(text)
                .raw(" OR TO ")
                .quoted(text)
                .raw(" BODY ")
                .quoted(text),
        )
    }
}

impl Completed {
    /// This result when it is `OK`, otherwise the matching error.
    ///
    /// # Errors
    ///
    /// [`Error::No`] or [`Error::Bad`] with the server's words.
    pub fn require_ok(self) -> Result<Self, Error> {
        match self.status {
            Status::Ok => Ok(self),
            Status::No => Err(Error::No(self.text)),
            Status::Bad => Err(Error::Bad(self.text)),
        }
    }
}

fn connector(config: &Config) -> Result<TlsConnector, Error> {
    let options = ClientOptions {
        extra_roots_pem: config
            .tls_roots_pem
            .iter()
            .map(|p| p.as_bytes().to_vec())
            .collect(),
        ..ClientOptions::default()
    };
    Ok(TlsConnector::new(
        quench_tls::client_config(&options).map_err(Error::Tls)?,
    ))
}

fn words(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_string).collect()
}

/// Only letters, digits, hyphens and spaces: header names for a fetch.
fn safe_words(fields: &str) -> String {
    fields
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == ' ')
        .collect()
}

/// A UID set is digits, colons, commas and `*`.
fn check_set(set: &str) -> Result<(), Error> {
    if !set.is_empty()
        && set.len() <= 4096
        && set
            .bytes()
            .all(|b| b.is_ascii_digit() || b",:*".contains(&b))
    {
        Ok(())
    } else {
        Err(Error::Protocol("bad UID set".into()))
    }
}

/// A flag is a `\System` flag or a keyword: atom characters only.
fn check_flag(flag: &str) -> Result<String, Error> {
    let body = flag.strip_prefix('\\').unwrap_or(flag);
    if !body.is_empty()
        && body.len() <= 64
        && body
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_$.".contains(&b))
    {
        Ok(flag.to_string())
    } else {
        Err(Error::Protocol("bad flag".into()))
    }
}

/// The length of the literal a line announces (`... {123}`), if it does.
fn literal_length(line: &[u8]) -> Option<usize> {
    let inner = line.strip_suffix(b"}")?;
    let open = inner.iter().rposition(|b| *b == b'{')?;
    let digits = &inner[open + 1..];
    let digits = digits.strip_suffix(b"+").unwrap_or(digits);
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(digits).ok()?.parse().ok()
}

fn parse_tagged(response: &[u8], tag: &str) -> Result<Option<Completed>, Error> {
    let Some(rest) = response.strip_prefix(tag.as_bytes()) else {
        return Ok(None);
    };
    let Some(rest) = rest.strip_prefix(b" ") else {
        return Ok(None);
    };
    let text = String::from_utf8_lossy(rest);
    let text = text.trim_end();
    let (word, tail) = text.split_once(' ').unwrap_or((text, ""));
    let status = match word.to_ascii_uppercase().as_str() {
        "OK" => Status::Ok,
        "NO" => Status::No,
        "BAD" => Status::Bad,
        _ => return Err(Error::Protocol("bad tagged response".into())),
    };
    let (code, text) = split_code(tail);
    Ok(Some(Completed {
        status,
        code,
        text,
        untagged: Vec::new(),
    }))
}

/// `[CODE value] text` -> (code, value), text.
fn split_code(text: &str) -> (Option<(String, String)>, String) {
    if let Some(rest) = text.strip_prefix('[')
        && let Some(close) = rest.find(']')
    {
        let inner = &rest[..close];
        let (name, value) = inner.split_once(' ').unwrap_or((inner, ""));
        return (
            Some((name.to_ascii_uppercase(), value.to_string())),
            rest[close + 1..].trim().to_string(),
        );
    }
    (None, text.to_string())
}

fn parse_untagged(response: &[u8]) -> Result<Untagged, Error> {
    let Some(rest) = response.strip_prefix(b"* ") else {
        return Ok(Untagged::Other(
            String::from_utf8_lossy(response).trim().to_string(),
        ));
    };
    let mut lexer = Lexer::new(rest);
    let Some(first) = lexer.value(0)? else {
        return Ok(Untagged::Other(String::new()));
    };
    let Some(first) = first.text() else {
        return Ok(Untagged::Other(String::new()));
    };
    if let Ok(number) = first.parse::<u32>() {
        let Some(kind) = lexer.value(0)?.and_then(|v| v.text()) else {
            return Ok(Untagged::Other(first));
        };
        return Ok(match kind.to_ascii_uppercase().as_str() {
            "EXISTS" => Untagged::Exists(number),
            "EXPUNGE" => Untagged::Expunge(number),
            "RECENT" => Untagged::Recent(number),
            "FETCH" => {
                let items = match lexer.value(0)? {
                    Some(Value::List(items)) => items,
                    _ => Vec::new(),
                };
                Untagged::Fetch { seq: number, items }
            }
            _ => Untagged::Other(format!("{first} {kind}")),
        });
    }
    match first.to_ascii_uppercase().as_str() {
        "OK" | "NO" | "BAD" | "PREAUTH" => {
            let status = match first.to_ascii_uppercase().as_str() {
                "NO" => Status::No,
                "BAD" => Status::Bad,
                _ => Status::Ok,
            };
            let text = String::from_utf8_lossy(&rest[lexer.at.min(rest.len())..])
                .trim()
                .to_string();
            let (code, text) = split_code(&text);
            Ok(Untagged::Condition { status, code, text })
        }
        "BYE" => Ok(Untagged::Bye(
            String::from_utf8_lossy(&rest[lexer.at.min(rest.len())..])
                .trim()
                .to_string(),
        )),
        "CAPABILITY" => {
            let text = String::from_utf8_lossy(&rest[lexer.at.min(rest.len())..]).into_owned();
            Ok(Untagged::Capability(words(&text)))
        }
        "FLAGS" => {
            let items = lexer.rest()?;
            let flags = items
                .first()
                .and_then(Value::list)
                .map(|l| l.iter().filter_map(Value::text).collect())
                .unwrap_or_default();
            Ok(Untagged::Flags(flags))
        }
        "SEARCH" => {
            let text = String::from_utf8_lossy(&rest[lexer.at.min(rest.len())..]).into_owned();
            Ok(Untagged::Search(
                text.split_whitespace()
                    .filter_map(|n| n.parse().ok())
                    .collect(),
            ))
        }
        "LIST" | "LSUB" => {
            let items = lexer.rest()?;
            if let [Value::List(attributes), delimiter, name, ..] = items.as_slice() {
                let attributes = attributes
                    .iter()
                    .filter_map(Value::text)
                    .map(|a| a.trim_start_matches('\\').to_ascii_lowercase())
                    .collect();
                let delimiter = delimiter.text().and_then(|d| d.chars().next());
                let name = name.text().unwrap_or_default();
                return Ok(Untagged::List(Folder {
                    path: utf7_decode(&name),
                    delimiter,
                    attributes,
                }));
            }
            Ok(Untagged::Other("LIST".into()))
        }
        "STATUS" => {
            let items = lexer.rest()?;
            let mailbox = items
                .first()
                .and_then(Value::text)
                .map(|n| utf7_decode(&n))
                .unwrap_or_default();
            let mut pairs = Vec::new();
            if let Some(Value::List(list)) = items.get(1) {
                for pair in list.chunks(2) {
                    if let [key, value] = pair
                        && let (Some(key), Some(value)) = (key.text(), value.number())
                    {
                        pairs.push((key.to_ascii_uppercase(), value));
                    }
                }
            }
            Ok(Untagged::Status {
                mailbox,
                items: pairs,
            })
        }
        _ => Ok(Untagged::Other(first)),
    }
}

fn collect_fetched(untagged: &[Untagged]) -> Vec<Fetched> {
    let mut out = Vec::new();
    for item in untagged {
        let Untagged::Fetch { items, .. } = item else {
            continue;
        };
        let mut fetched = Fetched {
            uid: 0,
            flags: Vec::new(),
            internal_date: None,
            size: 0,
            data: Vec::new(),
        };
        for pair in items.chunks(2) {
            let [key, value] = pair else { continue };
            let Some(key) = key.text() else { continue };
            let upper = key.to_ascii_uppercase();
            match upper.as_str() {
                "UID" => {
                    fetched.uid = value
                        .number()
                        .and_then(|n| u32::try_from(n).ok())
                        .unwrap_or(0)
                }
                "FLAGS" => {
                    fetched.flags = value
                        .list()
                        .map(|l| l.iter().filter_map(Value::text).collect())
                        .unwrap_or_default();
                }
                "INTERNALDATE" => {
                    fetched.internal_date = value.text().and_then(|t| parse_internal_date(&t))
                }
                "RFC822.SIZE" => {
                    fetched.size = value
                        .number()
                        .and_then(|n| u32::try_from(n).ok())
                        .unwrap_or(0)
                }
                _ if upper.starts_with("BODY[") || upper == "RFC822" => {
                    fetched.data = value.bytes().map(<[u8]>::to_vec).unwrap_or_default();
                }
                _ => {}
            }
        }
        if fetched.uid != 0 {
            out.push(fetched);
        }
    }
    out
}

/// `17-Jul-1996 02:44:25 -0700` as Unix seconds.
#[must_use]
pub fn parse_internal_date(text: &str) -> Option<i64> {
    let mut words = text.split_whitespace();
    let date = words.next()?;
    let time = words.next()?;
    let zone = words.next().unwrap_or("+0000");
    let mut parts = date.split('-');
    let day = parts.next()?;
    let month = parts.next()?;
    let year = parts.next()?;
    crate::date::parse(&format!("{day} {month} {year} {time} {zone}"))
}

/// Modified UTF-7 (RFC 3501 section 5.1.3) to Unicode.
#[must_use]
pub fn utf7_decode(name: &str) -> String {
    if !name.contains('&') {
        return name.to_string();
    }
    let mut out = String::new();
    let mut rest = name;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('-') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let encoded = &after[..end];
        if encoded.is_empty() {
            out.push('&');
        } else {
            let standard = encoded.replace(',', "/");
            let bytes = crate::encoding::base64_decode(standard.as_bytes());
            let units: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_be_bytes(*c))
                .collect();
            out.push_str(&String::from_utf16_lossy(&units));
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Unicode to modified UTF-7.
#[must_use]
pub fn utf7_encode(name: &str) -> String {
    let mut out = String::new();
    let mut pending: Vec<u16> = Vec::new();
    let flush = |pending: &mut Vec<u16>, out: &mut String| {
        if pending.is_empty() {
            return;
        }
        let bytes: Vec<u8> = pending.iter().flat_map(|u| u.to_be_bytes()).collect();
        let encoded = crate::encoding::base64_encode(&bytes);
        out.push('&');
        out.push_str(&encoded.trim_end_matches('=').replace('/', ","));
        out.push('-');
        pending.clear();
    };
    for c in name.chars() {
        if ('\u{20}'..='\u{7e}').contains(&c) {
            flush(&mut pending, &mut out);
            if c == '&' {
                out.push_str("&-");
            } else {
                out.push(c);
            }
        } else {
            let mut buffer = [0u16; 2];
            pending.extend_from_slice(c.encode_utf16(&mut buffer));
        }
    }
    flush(&mut pending, &mut out);
    out
}
