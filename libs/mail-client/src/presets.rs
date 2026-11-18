//! Where the common providers' mail servers are, so adding an account asks for an address and a password and not a host
//! list. Nothing here is fetched from the network: a table, and a guess for everything else.

/// How a connection is secured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Security {
    /// TLS from the first byte (IMAPS 993, SMTPS 465).
    Tls,
    /// Plain first, upgraded with STARTTLS before any credential is sent (143, 587).
    StartTls,
}

impl Security {
    /// The word stored and shown.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Tls => "tls",
            Self::StartTls => "starttls",
        }
    }

    /// The reverse of [`Security::name`].
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "tls" => Some(Self::Tls),
            "starttls" => Some(Self::StartTls),
            _ => None,
        }
    }
}

/// A provider's servers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    /// Address domains that belong to it.
    pub domains: &'static [&'static str],
    pub imap_host: &'static str,
    pub imap_port: u16,
    pub imap_security: Security,
    pub smtp_host: &'static str,
    pub smtp_port: u16,
    pub smtp_security: Security,
    /// What the person has to do first (an app password, a bridge), in English.
    pub note: &'static str,
}

/// Provider table. Hosts are the providers' published IMAP/SMTP settings.
pub const PRESETS: &[Preset] = &[
    Preset {
        id: "gmail",
        name: "Gmail",
        domains: &["gmail.com", "googlemail.com"],
        imap_host: "imap.gmail.com",
        imap_port: 993,
        imap_security: Security::Tls,
        smtp_host: "smtp.gmail.com",
        smtp_port: 465,
        smtp_security: Security::Tls,
        note: "Turn on 2-step verification, then create an app password (Google Account > Security > App passwords) and use it here.",
    },
    Preset {
        id: "outlook",
        name: "Outlook.com / Hotmail",
        domains: &[
            "outlook.com",
            "hotmail.com",
            "live.com",
            "msn.com",
            "outlook.pl",
            "hotmail.pl",
        ],
        imap_host: "outlook.office365.com",
        imap_port: 993,
        imap_security: Security::Tls,
        smtp_host: "smtp-mail.outlook.com",
        smtp_port: 587,
        smtp_security: Security::StartTls,
        note: "Needs an app password (Microsoft account > Security > App passwords). Work and school accounts often forbid this.",
    },
    Preset {
        id: "icloud",
        name: "iCloud Mail",
        domains: &["icloud.com", "me.com", "mac.com"],
        imap_host: "imap.mail.me.com",
        imap_port: 993,
        imap_security: Security::Tls,
        smtp_host: "smtp.mail.me.com",
        smtp_port: 587,
        smtp_security: Security::StartTls,
        note: "Use an app-specific password (appleid.apple.com).",
    },
    Preset {
        id: "yahoo",
        name: "Yahoo Mail",
        domains: &["yahoo.com", "ymail.com", "yahoo.pl"],
        imap_host: "imap.mail.yahoo.com",
        imap_port: 993,
        imap_security: Security::Tls,
        smtp_host: "smtp.mail.yahoo.com",
        smtp_port: 465,
        smtp_security: Security::Tls,
        note: "Create an app password in Account security.",
    },
    Preset {
        id: "fastmail",
        name: "Fastmail",
        domains: &["fastmail.com", "fastmail.fm"],
        imap_host: "imap.fastmail.com",
        imap_port: 993,
        imap_security: Security::Tls,
        smtp_host: "smtp.fastmail.com",
        smtp_port: 465,
        smtp_security: Security::Tls,
        note: "Create an app password (Settings > Privacy & Security > App passwords).",
    },
    Preset {
        id: "gmx",
        name: "GMX",
        domains: &["gmx.com", "gmx.net", "gmx.de"],
        imap_host: "imap.gmx.net",
        imap_port: 993,
        imap_security: Security::Tls,
        smtp_host: "mail.gmx.net",
        smtp_port: 465,
        smtp_security: Security::Tls,
        note: "Enable IMAP/SMTP access in the GMX settings first.",
    },
    Preset {
        id: "zoho",
        name: "Zoho Mail",
        domains: &["zoho.com", "zohomail.com"],
        imap_host: "imap.zoho.com",
        imap_port: 993,
        imap_security: Security::Tls,
        smtp_host: "smtp.zoho.com",
        smtp_port: 465,
        smtp_security: Security::Tls,
        note: "Enable IMAP access and use an app password if two-factor is on.",
    },
    Preset {
        id: "wp",
        name: "WP Poczta",
        domains: &["wp.pl"],
        imap_host: "imap.wp.pl",
        imap_port: 993,
        imap_security: Security::Tls,
        smtp_host: "smtp.wp.pl",
        smtp_port: 465,
        smtp_security: Security::Tls,
        note: "Enable POP3/IMAP in the WP settings first.",
    },
    Preset {
        id: "onet",
        name: "Onet Poczta",
        domains: &["onet.pl", "onet.eu", "op.pl", "vp.pl", "autograf.pl"],
        imap_host: "imap.poczta.onet.pl",
        imap_port: 993,
        imap_security: Security::Tls,
        smtp_host: "smtp.poczta.onet.pl",
        smtp_port: 465,
        smtp_security: Security::Tls,
        note: "Enable IMAP in the Onet settings first.",
    },
    Preset {
        id: "o2",
        name: "o2 Poczta",
        domains: &["o2.pl", "tlen.pl"],
        imap_host: "poczta.o2.pl",
        imap_port: 993,
        imap_security: Security::Tls,
        smtp_host: "poczta.o2.pl",
        smtp_port: 465,
        smtp_security: Security::Tls,
        note: "Enable IMAP in the o2 settings first.",
    },
    Preset {
        id: "interia",
        name: "Interia Poczta",
        domains: &["interia.pl", "interia.eu", "poczta.fm", "vp.pl"],
        imap_host: "poczta.interia.pl",
        imap_port: 993,
        imap_security: Security::Tls,
        smtp_host: "poczta.interia.pl",
        smtp_port: 465,
        smtp_security: Security::Tls,
        note: "Enable IMAP in the Interia settings first.",
    },
];

/// Proton has no public IMAP: Proton Mail Bridge, running somewhere Mailroom can reach, exposes it. These are the
/// Bridge's default ports; the host is wherever the Bridge runs, and must be on the administrator's private-host list.
pub const PROTON_BRIDGE: Preset = Preset {
    id: "proton-bridge",
    name: "Proton Mail (via Bridge)",
    domains: &["proton.me", "protonmail.com", "pm.me", "protonmail.ch"],
    imap_host: "proton-bridge",
    imap_port: 1143,
    imap_security: Security::StartTls,
    smtp_host: "proton-bridge",
    smtp_port: 1025,
    smtp_security: Security::StartTls,
    note: "Proton offers no direct IMAP. Run Proton Mail Bridge, sign in there, and use the Bridge's own mailbox password. The Bridge host must be allowed by an administrator, and its certificate pasted in the account.",
};

/// The preset for an address (by its domain), if the table knows the provider.
#[must_use]
pub fn for_address(address: &str) -> Option<&'static Preset> {
    let domain = address.rsplit_once('@')?.1.trim().to_ascii_lowercase();
    if PROTON_BRIDGE.domains.contains(&domain.as_str()) {
        return Some(&PROTON_BRIDGE);
    }
    PRESETS
        .iter()
        .find(|p| p.domains.contains(&domain.as_str()))
}

/// Hosts to try for a domain the table does not know: `imap.<domain>` and `smtp.<domain>` on the standard TLS ports.
#[must_use]
pub fn guess(address: &str) -> Option<(String, String)> {
    let domain = address.rsplit_once('@')?.1.trim().to_ascii_lowercase();
    crate::netguard::valid_host(&domain)
        .then(|| (format!("imap.{domain}"), format!("smtp.{domain}")))
}
