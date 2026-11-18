//! A connection that may or may not be TLS yet, and the server's TLS setup.

use quench_tls::{TlsAcceptor, TlsStream};
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::TcpStream;

/// The certificate the servers present: `CN=localhost`, valid for the name `localhost`. Trust it with
/// [`CERT_PEM`] and connect with the TLS server name `localhost`.
pub const CERT_PEM: &str = include_str!("../fixtures/localhost.crt");
const KEY_PEM: &str = include_str!("../fixtures/localhost.key");

/// The acceptor for the fixture certificate.
///
/// # Panics
///
/// Never in practice: the fixture is a valid pair.
#[must_use]
pub fn acceptor() -> TlsAcceptor {
    quench_tls::install_default_provider();
    let chain = quench_tls::pem_certificates(CERT_PEM.as_bytes()).expect("fixture certificate");
    let key = quench_tls::pem_private_key(KEY_PEM.as_bytes()).expect("fixture key");
    let config = quench_tls::server_config(chain, key).expect("fixture server config");
    TlsAcceptor::new(Arc::new(config))
}

pub enum Stream {
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
            Self::Closed => Poll::Ready(Ok(())),
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
            Self::Closed => Poll::Ready(Err(io::ErrorKind::BrokenPipe.into())),
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

/// A buffered line reader over a [`Stream`].
pub struct Conn {
    pub stream: Stream,
    pub buffer: Vec<u8>,
}

impl Conn {
    pub const fn new(stream: Stream) -> Self {
        Self {
            stream,
            buffer: Vec::new(),
        }
    }

    /// One line including its `\n`; `None` at end of stream.
    pub async fn line(&mut self) -> Option<Vec<u8>> {
        loop {
            if let Some(i) = self.buffer.iter().position(|b| *b == b'\n') {
                let rest = self.buffer.split_off(i + 1);
                return Some(std::mem::replace(&mut self.buffer, rest));
            }
            let mut chunk = [0u8; 8192];
            match self.stream.read(&mut chunk).await {
                Ok(0) | Err(_) => return None,
                Ok(n) => self.buffer.extend_from_slice(&chunk[..n]),
            }
        }
    }

    /// Exactly `n` bytes.
    pub async fn exact(&mut self, n: usize) -> Option<Vec<u8>> {
        while self.buffer.len() < n {
            let mut chunk = [0u8; 8192];
            match self.stream.read(&mut chunk).await {
                Ok(0) | Err(_) => return None,
                Ok(k) => self.buffer.extend_from_slice(&chunk[..k]),
            }
        }
        let rest = self.buffer.split_off(n);
        Some(std::mem::replace(&mut self.buffer, rest))
    }

    pub async fn send(&mut self, data: &[u8]) -> bool {
        self.stream.write_all(data).await.is_ok() && self.stream.flush().await.is_ok()
    }

    /// Upgrades a plain connection to TLS (the server side of STARTTLS).
    pub async fn upgrade(&mut self, acceptor: &TlsAcceptor) -> bool {
        let Stream::Plain(tcp) = std::mem::replace(&mut self.stream, Stream::Closed) else {
            return false;
        };
        match acceptor.accept(tcp).await {
            Ok(tls) => {
                self.stream = Stream::Tls(Box::new(tls));
                true
            }
            Err(_) => false,
        }
    }

    pub const fn is_tls(&self) -> bool {
        matches!(self.stream, Stream::Tls(_))
    }
}
