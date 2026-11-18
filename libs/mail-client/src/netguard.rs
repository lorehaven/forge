//! Which addresses Mailroom may connect to.
//!
//! A person who adds a mailbox chooses the host Mailroom dials, from inside the cluster. Unchecked, that is a way to reach
//! everything the pod can reach: the cluster API, Postgres, Redis, the cloud metadata address, other services' admin ports.
//! So connections go to **public** addresses only, on **mail** ports only, unless an administrator allows a specific private
//! host (an in-cluster Proton Bridge). The check is made on the addresses the name resolves to *and the connection is made to
//! those very addresses*, so a name that changes its answer between check and connect (DNS rebinding) gains nothing.

use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;
use tokio::net::TcpStream;

/// Ports a user's mail server may listen on: IMAP, IMAPS, SMTP submission, SMTPS, alternative submission.
pub const MAIL_PORTS: &[u16] = &[143, 993, 465, 587, 2525];

/// What an administrator has allowed beyond the public internet.
#[derive(Debug, Clone, Default)]
pub struct Policy {
    /// Hosts allowed to resolve to private addresses (exact names, or `*.suffix` for a domain and everything under it).
    pub private_hosts: Vec<String>,
    /// Extra ports allowed besides [`MAIL_PORTS`].
    pub extra_ports: Vec<u16>,
    /// How long to wait for a connection.
    pub connect_timeout: Option<Duration>,
}

impl Policy {
    /// Whether `host` is on the private allow-list.
    #[must_use]
    pub fn allows_private(&self, host: &str) -> bool {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        self.private_hosts.iter().any(|rule| {
            let rule = rule.trim().trim_end_matches('.').to_ascii_lowercase();
            rule.strip_prefix("*.").map_or_else(
                || rule == host,
                |suffix| host == suffix || host.ends_with(&format!(".{suffix}")),
            )
        })
    }

    /// Whether `port` may be dialled.
    #[must_use]
    pub fn allows_port(&self, port: u16) -> bool {
        MAIL_PORTS.contains(&port) || self.extra_ports.contains(&port)
    }
}

/// Why a destination was refused. The text is safe to show the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    BadHost,
    Port(u16),
    /// The name has no address, or the lookup failed.
    NoAddress,
    /// The name resolves to an address that is not public, and the host is not on the allow-list.
    NotPublic,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::BadHost => "that is not a valid host name",
            Self::Port(_) => "that port is not one a mail server uses",
            Self::NoAddress => "the host name does not resolve",
            Self::NotPublic => "that host is not on the public internet",
        })
    }
}

impl std::error::Error for Refusal {}

/// A host name or IP literal: letters, digits, dots, hyphens, colons for IPv6; no scheme, no path, no spaces.
#[must_use]
pub fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '_' | '[' | ']'))
        && !host.starts_with(['.', '-'])
}

/// Whether an address belongs on the public internet: not loopback, private, link-local, shared (CGNAT), documentation,
/// benchmarking, multicast, reserved or unspecified, and, for IPv6, not an IPv4 address wrapped in a transition format
/// whose IPv4 side would be refused.
#[must_use]
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => public_v4(v4),
        IpAddr::V6(v6) => public_v6(v6),
    }
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0
        || a == 10
        || a == 127
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 192 && b == 88 && c == 99)
        || (a == 192 && b == 168)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || a >= 224)
}

fn public_v6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    // IPv4-mapped (::ffff:a.b.c.d) and the deprecated IPv4-compatible form: judge the IPv4 address inside.
    if let Some(v4) = ip.to_ipv4_mapped() {
        return public_v4(v4);
    }
    let embedded = |high: u16, low: u16| {
        Ipv4Addr::new((high >> 8) as u8, high as u8, (low >> 8) as u8, low as u8)
    };
    // NAT64 64:ff9b::/96 and 6to4 2002::/16 carry an IPv4 address; Teredo 2001::/32 is refused outright.
    if segments[0] == 0x0064 && segments[1] == 0xff9b && segments[2..6].iter().all(|s| *s == 0) {
        return public_v4(embedded(segments[6], segments[7]));
    }
    if segments[0] == 0x2002 {
        return public_v4(embedded(segments[1], segments[2]));
    }
    if segments[0] == 0x2001 && segments[1] == 0 {
        return false;
    }
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00 // unique local fc00::/7
        || (segments[0] & 0xffc0) == 0xfe80 // link-local fe80::/10
        || (segments[0] & 0xffc0) == 0xfec0 // site-local, deprecated
        || (segments[0] == 0x2001 && segments[1] == 0x0db8) // documentation
        || (segments[0] == 0x3fff && segments[1] < 0x1000) // documentation 3fff::/20
        || segments[0] == 0x0100 && segments[1..4].iter().all(|s| *s == 0)) // discard-only
        && (ip.segments()[0] & 0xe000) == 0x2000 // only global unicast 2000::/3 is ever public
}

/// Resolves `host` and checks every address it answers with. Returns the addresses to connect to, IPv4 first (all of them passed).
///
/// # Errors
///
/// A [`Refusal`] when the host, port or any resolved address is not allowed.
pub async fn resolve(host: &str, port: u16, policy: &Policy) -> Result<Vec<SocketAddr>, Refusal> {
    let host = host.trim().trim_start_matches('[').trim_end_matches(']');
    if !valid_host(host) {
        return Err(Refusal::BadHost);
    }
    if !policy.allows_port(port) {
        return Err(Refusal::Port(port));
    }
    let addresses: Vec<SocketAddr> = if let Ok(ip) = host.parse::<IpAddr>() {
        vec![SocketAddr::new(ip, port)]
    } else {
        tokio::net::lookup_host((host, port))
            .await
            .map_err(|_| Refusal::NoAddress)?
            .collect()
    };
    if addresses.is_empty() {
        return Err(Refusal::NoAddress);
    }
    let private_ok = policy.allows_private(host);
    if !private_ok && addresses.iter().any(|a| !is_public(a.ip())) {
        return Err(Refusal::NotPublic);
    }
    // IPv4 first: a cluster with no IPv6 route should not wait out a timeout on every connection.
    let mut addresses = addresses;
    addresses.sort_by_key(SocketAddr::is_ipv6);
    Ok(addresses)
}

/// Resolves, checks and connects, to the addresses that were checked.
///
/// # Errors
///
/// A refusal (as `PermissionDenied`) or the connection error of the last address tried.
pub async fn connect(host: &str, port: u16, policy: &Policy) -> io::Result<TcpStream> {
    let addresses = resolve(host, port, policy)
        .await
        .map_err(|r| io::Error::new(io::ErrorKind::PermissionDenied, r))?;
    let timeout = policy.connect_timeout.unwrap_or(Duration::from_secs(15));
    let mut last = io::Error::new(io::ErrorKind::NotFound, "no address");
    for address in addresses {
        match tokio::time::timeout(timeout, TcpStream::connect(address)).await {
            Ok(Ok(stream)) => {
                let _ = stream.set_nodelay(true);
                return Ok(stream);
            }
            Ok(Err(e)) => last = e,
            Err(_) => last = io::Error::new(io::ErrorKind::TimedOut, "connection timed out"),
        }
    }
    Err(last)
}
