use anyhow::{Context, Result};
use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct Conversation {
    pub id: Uuid,
    pub name: String,
    pub created: DateTime<Utc>,
    pub messages: Vec<Value>,
}

pub fn sessions_dir() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("Cannot get current directory")?;
    let dir = cwd.join(".ferrous").join("sessions");
    fs::create_dir_all(&dir).context("Cannot create sessions directory")?;
    Ok(dir)
}

#[must_use]
pub fn sanitize_name(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();

    cleaned
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join("-")
        .trim_matches('-')
        .to_string()
}

#[must_use]
pub fn generate_filename(name: &str, id: Uuid) -> String {
    let timestamp = Local::now().format("%Y%m%dT%H%M");
    let short_id = id.to_string()[..8].to_string();
    let safe_name = if name.trim().is_empty() {
        format!("unnamed-{timestamp}")
    } else {
        sanitize_name(name)
    };
    format!("{timestamp}_{safe_name}_{short_id}.json")
}

pub fn save_conversation(messages: &[Value], name: &str) -> Result<(String, PathBuf)> {
    let id = Uuid::new_v4();
    let filename = generate_filename(name, id);
    let path = sessions_dir()?.join(&filename);

    let conv = Conversation {
        id,
        name: name.trim().to_string(),
        created: Utc::now(),
        messages: messages.to_vec(),
    };

    let json = serde_json::to_string_pretty(&conv).context("Failed to serialize conversation")?;

    fs::write(&path, json).context("Failed to write conversation file")?;

    Ok((filename, path))
}

pub fn load_conversation_by_prefix(prefix: &str) -> Result<(Conversation, PathBuf)> {
    let dir = sessions_dir()?;
    let mut candidates = vec![];

    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        let content = fs::read_to_string(&path)?;
        let conv: Conversation = serde_json::from_str(&content)?;

        let filename = path.file_name().unwrap().to_string_lossy().to_string();

        if filename.starts_with(prefix)
            || conv.name.to_lowercase().starts_with(&prefix.to_lowercase())
            || conv.id.to_string().starts_with(prefix)
            || conv.id.to_string()[..8].starts_with(prefix)
        {
            candidates.push((conv, path));
        }
    }

    match candidates.len() {
        0 => Err(anyhow::anyhow!("No conversation found matching '{prefix}'")),
        1 => Ok(candidates.into_iter().next().unwrap()),
        n => Err(anyhow::anyhow!(
            "Ambiguous prefix '{}'. {} matches found:\n  {}",
            prefix,
            n,
            candidates
                .iter()
                .map(|(c, p)| format!("{} ({})", c.name, p.file_name().unwrap().to_string_lossy()))
                .collect::<Vec<_>>()
                .join("\n  ")
        )),
    }
}

pub fn list_conversations() -> Result<Vec<(String, String, String)>> {
    let dir = sessions_dir()?;
    let mut items = vec![];

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        let content = fs::read_to_string(&path)?;
        let conv: Conversation = serde_json::from_str(&content)?;

        let short_id = conv.id.to_string()[..8].to_string();
        let date = conv.created.with_timezone(&Local).format("%Y-%m-%d %H:%M");
        items.push((conv.name, short_id, date.to_string()));
    }

    items.sort_by(|a, b| b.2.cmp(&a.2)); // newest first
    Ok(items)
}

pub fn delete_conversation_by_prefix(prefix: &str) -> Result<String> {
    let (conv, path) = load_conversation_by_prefix(prefix)?;
    fs::remove_file(&path)?;
    Ok(conv.name)
}
