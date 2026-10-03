//! `riveter secrets ...` as `main` runs it.

use crate::cli::SecretsCmd;
use crate::env::overlay_dir;
use crate::repl::{ok, warn};
use crate::secrets::{SyncRequest, sync};
use crate::vault::{self, Vault};
use anyhow::{Context as _, Result, bail, ensure};
use std::io::Read as _;
use std::path::Path;

/// `riveter secrets ...`.
pub fn secrets_command(cmd: &SecretsCmd) -> Result<()> {
    match cmd {
        SecretsCmd::Keygen { out } => keygen(out.as_deref()),
        SecretsCmd::Set {
            file,
            name,
            value,
            recipients,
        } => set(file, name, value.as_deref(), recipients),
        SecretsCmd::Import {
            file,
            from,
            names,
            recipients,
        } => import(file, from, names, recipients),
        SecretsCmd::List { file } => {
            for name in Vault::read(file)?.names() {
                println!("{name}");
            }
            Ok(())
        }
        SecretsCmd::Show { file, name } => {
            println!("{}", Vault::read(file)?.get(&identities()?, name)?);
            Ok(())
        }
        SecretsCmd::Remove { file, name } => remove(file, name),
        SecretsCmd::Rekey { file, recipients } => rekey(file, recipients),
        SecretsCmd::Sync {
            overlays,
            all,
            namespace,
            context,
            dry_run,
        } => {
            let synced = sync(
                &SyncRequest {
                    overlays: overlays.clone(),
                    all: *all,
                    namespace: namespace.clone(),
                    context: context.clone(),
                    dry_run: *dry_run,
                },
                &overlay_dir(),
            )?;

            for s in &synced {
                let verb = if *dry_run { "would sync" } else { "synced" };
                ok(&format!(
                    "{verb} {} ({}): {} variable(s): {}",
                    s.secret,
                    s.namespace,
                    s.variables.len(),
                    s.variables.join(", ")
                ));
            }
            if *dry_run {
                warn("dry run: nothing was written to the cluster");
            }
            Ok(())
        }
    }
}

fn identities() -> Result<Vec<age::x25519::Identity>> {
    vault::identities_from_env()?.with_context(|| {
        format!(
            "no key to open it: set {} to the key, or {} to a file holding it",
            vault::KEY_VAR,
            vault::KEY_FILE_VAR
        )
    })
}

/// The file to add values to: the existing one, with its own recipients, or a new one encrypting to
/// `--recipient` or, failing that, the configured key's own public half.
fn open_for_writing(file: &Path, flagged: &[String]) -> Result<Vault> {
    if file.exists() {
        let vault = Vault::read(file)?;
        ensure!(
            flagged.is_empty() || flagged.iter().all(|r| vault.recipients.contains(r)),
            "{} already encrypts to other recipients; use `riveter secrets rekey` to change them",
            file.display()
        );
        return Ok(vault);
    }

    let recipients = if flagged.is_empty() {
        match vault::identities_from_env()? {
            Some(identities) => identities
                .iter()
                .map(|i| i.to_public().to_string())
                .collect(),
            None => bail!(
                "a new file needs recipients: pass --recipient age1..., or configure a key ({} / {}) to encrypt to",
                vault::KEY_VAR,
                vault::KEY_FILE_VAR
            ),
        }
    } else {
        flagged.to_vec()
    };
    Ok(Vault::new(recipients))
}

fn read_stdin() -> Result<String> {
    let mut value = String::new();
    std::io::stdin().read_to_string(&mut value)?;
    // One trailing newline is the shell's, not the secret's.
    if value.ends_with('\n') {
        value.pop();
        if value.ends_with('\r') {
            value.pop();
        }
    }
    ensure!(
        !value.is_empty(),
        "no value on standard input (or pass --value)"
    );
    Ok(value)
}

fn keygen(out: Option<&Path>) -> Result<()> {
    let (secret, public) = vault::keygen();
    let text = format!(
        "# created: {}\n# public key: {public}\n{secret}\n",
        chrono::Utc::now().to_rfc3339()
    );
    match out {
        Some(path) => {
            ensure!(
                !path.exists(),
                "{} already exists; refusing to overwrite a key",
                path.display()
            );
            if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
                std::fs::create_dir_all(dir)?;
            }
            write_private(path, &text)?;
            ok(&format!(
                "private key written to {} (keep it out of git)",
                path.display()
            ));
        }
        None => print!("{text}"),
    }
    println!("{public}");
    Ok(())
}

#[cfg(unix)]
fn write_private(path: &Path, text: &str) -> Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("failed to create {}", path.display()))?;
    file.write_all(text.as_bytes())?;
    Ok(())
}

#[cfg(not(unix))]
fn write_private(path: &Path, text: &str) -> Result<()> {
    std::fs::write(path, text).with_context(|| format!("failed to write {}", path.display()))
}

fn set(file: &Path, name: &str, value: Option<&str>, recipients: &[String]) -> Result<()> {
    let value = match value {
        Some(value) => value.to_string(),
        None => read_stdin()?,
    };
    let mut vault = open_for_writing(file, recipients)?;
    vault.set(name, &value)?;
    vault.write(file)?;
    ok(&format!("{name} encrypted into {}", file.display()));
    Ok(())
}

fn import(file: &Path, from: &Path, names: &[String], recipients: &[String]) -> Result<()> {
    let text = std::fs::read_to_string(from)
        .with_context(|| format!("failed to read {}", from.display()))?;
    let all = crate::render::parse_dotenv(&text);
    for name in names {
        ensure!(
            all.contains_key(name),
            "`{name}` is not in {}",
            from.display()
        );
    }
    let mut chosen: Vec<(&String, &String)> = all
        .iter()
        .filter(|(name, _)| names.is_empty() || names.contains(name))
        .collect();
    chosen.sort();
    ensure!(
        !chosen.is_empty(),
        "nothing to import from {}",
        from.display()
    );

    let mut vault = open_for_writing(file, recipients)?;
    for (name, value) in &chosen {
        vault.set(name, value)?;
    }
    vault.write(file)?;
    ok(&format!(
        "{} value(s) encrypted into {}: {}",
        chosen.len(),
        file.display(),
        chosen
            .iter()
            .map(|(n, _)| n.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    Ok(())
}

fn remove(file: &Path, name: &str) -> Result<()> {
    let mut vault = Vault::read(file)?;
    ensure!(
        vault.data.remove(name).is_some(),
        "`{name}` is not in {}",
        file.display()
    );
    vault.write(file)?;
    ok(&format!("{name} removed from {}", file.display()));
    Ok(())
}

fn rekey(file: &Path, recipients: &[String]) -> Result<()> {
    let mut vault = Vault::read(file)?;
    vault.rekey(&identities()?, recipients.to_vec())?;
    vault.write(file)?;
    ok(&format!(
        "{} value(s) in {} now encrypted to {} recipient(s)",
        vault.data.len(),
        file.display(),
        recipients.len()
    ));
    Ok(())
}
