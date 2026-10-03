//! Encrypted values: a YAML file of `NAME: ENC[age,...]` that is safe to commit.
//!
//! The names are readable and every value is encrypted on its own with [age](https://age-encryption.org) to
//! one or more recipients, so changing one secret changes one line of the file and a diff says *which*
//! secret moved without saying what it is. It is not SOPS: riveter reads and writes this itself, so nothing
//! else has to be installed, wherever riveter runs - including the runner image. Each value is plain age
//! ciphertext (base64 inside the `ENC[age,...]` wrapper), so one can still be decrypted by hand with the
//! `age` tool if riveter is ever not to hand.
//!
//! ```yaml
//! riveter-secrets: 1
//! recipients:
//!   - age1qyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqs3290gq
//! data:
//!   DB_PASSWORD: ENC[age,YWdlLWVuY3J5cHRpb24ub3JnL3Yx...]
//! ```
//!
//! The key that opens it is not in the file and not in git: `RIVETER_AGE_KEY` (the key itself) or
//! `RIVETER_AGE_KEY_FILE` (a file holding it, which can be mounted wherever riveter runs), failing those
//! `~/.config/riveter/age.key`.

use age::secrecy::ExposeSecret;
use anyhow::{Context as _, Result, bail, ensure};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// The file's name inside an overlay, and so inside its package.
pub const FILE: &str = "secrets.yaml";
const MARKER: &str = "riveter-secrets";
const VERSION: u32 = 1;
const PREFIX: &str = "ENC[age,";

/// Where the key can be named.
pub const KEY_VAR: &str = "RIVETER_AGE_KEY";
pub const KEY_FILE_VAR: &str = "RIVETER_AGE_KEY_FILE";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vault {
    #[serde(rename = "riveter-secrets", default = "version")]
    version: u32,
    /// Public keys every value is encrypted to. Anyone holding the matching private key can open the file.
    #[serde(default)]
    pub recipients: Vec<String>,
    /// Name to `ENC[age,...]`. Sorted, so the file is stable.
    #[serde(default)]
    pub data: BTreeMap<String, String>,
}

const fn version() -> u32 {
    VERSION
}

/// A new age key pair: the private key (`AGE-SECRET-KEY-1...`) and its public recipient (`age1...`).
#[must_use]
pub fn keygen() -> (String, String) {
    let identity = age::x25519::Identity::generate();
    (
        identity.to_string().expose_secret().to_string(),
        identity.to_public().to_string(),
    )
}

/// Private keys from a text: one `AGE-SECRET-KEY-1...` per line, `#` comments and blank lines ignored (the
/// format `age-keygen` writes).
pub fn parse_identities(text: &str) -> Result<Vec<age::x25519::Identity>> {
    let identities: Vec<age::x25519::Identity> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            age::x25519::Identity::from_str(line)
                .map_err(|e| anyhow::anyhow!("not an age secret key: {e}"))
        })
        .collect::<Result<_>>()?;
    ensure!(!identities.is_empty(), "no age secret key found");
    Ok(identities)
}

/// The key riveter should open files with, if one is named: [`KEY_VAR`], else [`KEY_FILE_VAR`], else the
/// default file. `Ok(None)` when none is configured at all.
pub fn identities_from_env() -> Result<Option<Vec<age::x25519::Identity>>> {
    identities_from(lookup)
}

/// A variable from the process environment.
///
/// For the two that name the key, failing that from a `.env` in the working directory - which is where an
/// estate keeps it, outside git. Nothing else is ever read from that file here: an install is not a function
/// of whichever directory it ran from, but where the key is kept is not part of what it installs.
#[must_use]
pub fn lookup(name: &str) -> Option<String> {
    let set = |v: String| Some(v).filter(|v| !v.trim().is_empty());
    if let Some(value) = std::env::var(name).ok().and_then(set) {
        return Some(value);
    }
    if name != KEY_VAR && name != KEY_FILE_VAR {
        return None;
    }
    let text = std::fs::read_to_string(".env").ok()?;
    crate::render::parse_dotenv(&text)
        .remove(name)
        .and_then(set)
}

/// [`identities_from_env`] with the environment passed in.
pub fn identities_from(
    get: impl Fn(&str) -> Option<String>,
) -> Result<Option<Vec<age::x25519::Identity>>> {
    if let Some(key) = get(KEY_VAR) {
        return parse_identities(&key)
            .context(format!("{KEY_VAR} is set but is not an age secret key"))
            .map(Some);
    }
    let path = match get(KEY_FILE_VAR) {
        Some(path) => PathBuf::from(path),
        None => match get("HOME") {
            Some(home) => Path::new(&home).join(".config/riveter/age.key"),
            None => return Ok(None),
        },
    };
    if !path.is_file() {
        // A file named explicitly that is not there is an error; the default being absent is just "no key".
        ensure!(
            get(KEY_FILE_VAR).is_none(),
            "{KEY_FILE_VAR} names {}, which is not a file",
            path.display()
        );
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    parse_identities(&text)
        .with_context(|| format!("{} is not an age key file", path.display()))
        .map(Some)
}

fn parse_recipients(recipients: &[String]) -> Result<Vec<age::x25519::Recipient>> {
    ensure!(
        !recipients.is_empty(),
        "no recipient: pass --recipient age1..., or have a key configured to encrypt to"
    );
    recipients
        .iter()
        .map(|r| {
            age::x25519::Recipient::from_str(r.trim())
                .map_err(|e| anyhow::anyhow!("`{r}` is not an age recipient: {e}"))
        })
        .collect()
}

/// Encrypts one value to every recipient, as `ENC[age,...]`.
pub fn encrypt_value(recipients: &[String], plaintext: &str) -> Result<String> {
    use std::io::Write as _;

    let parsed = parse_recipients(recipients)?;
    let refs = parsed.iter().map(|r| r as &dyn age::Recipient);
    let encryptor = age::Encryptor::with_recipients(refs).map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut ciphertext = Vec::new();
    let mut writer = encryptor
        .wrap_output(&mut ciphertext)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    writer.write_all(plaintext.as_bytes())?;
    writer.finish()?;
    Ok(format!("{PREFIX}{}]", STANDARD.encode(ciphertext)))
}

/// Opens one `ENC[age,...]` value with any of `identities`.
pub fn decrypt_value(identities: &[age::x25519::Identity], value: &str) -> Result<String> {
    use std::io::Read as _;

    let encoded = value
        .strip_prefix(PREFIX)
        .and_then(|rest| rest.strip_suffix(']'))
        .context("not an ENC[age,...] value")?;
    let ciphertext = STANDARD
        .decode(encoded)
        .context("the ciphertext is not valid base64")?;

    let decryptor =
        age::Decryptor::new(ciphertext.as_slice()).map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut reader = decryptor
        .decrypt(identities.iter().map(|i| i as &dyn age::Identity))
        .map_err(|_| {
            anyhow::anyhow!(
                "this key does not open the value (it was encrypted to other recipients)"
            )
        })?;
    let mut plaintext = String::new();
    reader
        .read_to_string(&mut plaintext)
        .context("the value is not valid UTF-8")?;
    Ok(plaintext)
}

impl Vault {
    /// Reads and checks a file: right marker, every value of the form `ENC[age,...]` - so a plaintext value
    /// can never be mistaken for a secret that was encrypted.
    pub fn parse(text: &str) -> Result<Self> {
        let vault: Self = serde_yaml::from_str(text).context("secrets.yaml is not valid YAML")?;
        ensure!(
            text.contains(MARKER),
            "this is not a riveter secrets file (no `{MARKER}` marker)"
        );
        ensure!(
            vault.version == VERSION,
            "secrets file version {} is not supported (this riveter reads {VERSION})",
            vault.version
        );
        for recipient in &vault.recipients {
            age::x25519::Recipient::from_str(recipient)
                .map_err(|e| anyhow::anyhow!("recipient `{recipient}`: {e}"))?;
        }
        for (name, value) in &vault.data {
            ensure!(
                value.starts_with(PREFIX) && value.ends_with(']'),
                "`{name}` is not encrypted (a value must be ENC[age,...]); add it with `riveter secrets set`"
            );
        }
        Ok(vault)
    }

    pub fn read(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        Self::parse(&text).with_context(|| path.display().to_string())
    }

    /// The file as written: a header saying what it is, then the data.
    pub fn render(&self) -> Result<String> {
        Ok(format!(
            "# Encrypted with age: committed, and opened only with the key named by {KEY_VAR} or {KEY_FILE_VAR}.\n\
             # Change it with `riveter secrets set|import|rekey`, not by hand.\n{}",
            serde_yaml::to_string(self)?
        ))
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        std::fs::write(path, self.render()?)
            .with_context(|| format!("failed to write {}", path.display()))
    }

    /// An empty vault that encrypts to `recipients`.
    #[must_use]
    pub const fn new(recipients: Vec<String>) -> Self {
        Self {
            version: VERSION,
            recipients,
            data: BTreeMap::new(),
        }
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.data.keys().map(String::as_str)
    }

    /// Encrypts `value` under `name`, replacing what was there. Every other value is left byte for byte.
    pub fn set(&mut self, name: &str, value: &str) -> Result<()> {
        ensure!(
            !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-'),
            "`{name}` is not a usable variable name"
        );
        let encrypted = encrypt_value(&self.recipients, value)?;
        self.data.insert(name.to_string(), encrypted);
        Ok(())
    }

    pub fn get(&self, identities: &[age::x25519::Identity], name: &str) -> Result<String> {
        let value = self
            .data
            .get(name)
            .with_context(|| format!("`{name}` is not in the file"))?;
        decrypt_value(identities, value).with_context(|| format!("`{name}`"))
    }

    /// Every value, opened.
    pub fn decrypt_all(
        &self,
        identities: &[age::x25519::Identity],
    ) -> Result<HashMap<String, String>> {
        self.data
            .iter()
            .map(|(name, value)| {
                decrypt_value(identities, value)
                    .map(|plain| (name.clone(), plain))
                    .with_context(|| format!("`{name}`"))
            })
            .collect()
    }

    /// Encrypts everything to `recipients` instead - after a key is lost or a person leaves. Needs a key that
    /// opens the file now.
    pub fn rekey(
        &mut self,
        identities: &[age::x25519::Identity],
        recipients: Vec<String>,
    ) -> Result<()> {
        let plain = self.decrypt_all(identities)?;
        ensure!(!recipients.is_empty(), "no recipients to encrypt to");
        self.recipients = recipients;
        for (name, value) in plain {
            self.set(&name, &value)?;
        }
        Ok(())
    }
}

/// The variables a package's `secrets.yaml` provides, opened with the configured key.
///
/// `Ok(None)` if the package has no such file. If it has one and no key is configured the error says how to
/// name one, instead of a variable later turning up undefined with no hint why.
pub fn variables(
    text: &str,
    identities: Option<&[age::x25519::Identity]>,
) -> Result<HashMap<String, String>> {
    let vault = Vault::parse(text)?;
    if vault.data.is_empty() {
        return Ok(HashMap::new());
    }
    let Some(identities) = identities else {
        bail!(
            "this overlay has encrypted values ({FILE}) but no key to open them: set {KEY_VAR}, or {KEY_FILE_VAR} to a key file"
        );
    };
    vault.decrypt_all(identities)
}
