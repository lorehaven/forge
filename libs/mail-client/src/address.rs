//! Address lists (`From`, `To`, `Cc`, `Reply-To`) read the way mail clients read them: leniently, for display.

use crate::encoding::decode_words;

/// A person: a name (possibly empty) and an address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    pub name: String,
    pub address: String,
}

impl Contact {
    /// `Name <address>`, or just the address when there is no name.
    #[must_use]
    pub fn display(&self) -> String {
        if self.name.is_empty() {
            self.address.clone()
        } else {
            format!("{} <{}>", self.name, self.address)
        }
    }
}

/// Most contacts read from one field.
const MAX_CONTACTS: usize = 500;

/// Reads an address list. Groups (`undisclosed-recipients:;`) contribute their members; anything that is not an address
/// is skipped. Encoded words in names are decoded.
#[must_use]
pub fn parse_list(value: &str) -> Vec<Contact> {
    let mut out = Vec::new();
    for item in split_top_level(value) {
        if out.len() >= MAX_CONTACTS {
            break;
        }
        if let Some(contact) = parse_one(&item) {
            out.push(contact);
        }
    }
    out
}

/// Splits on commas and semicolons that are not inside quotes, angle brackets or comments. A group's `name:` prefix is dropped.
fn split_top_level(value: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let (mut quoted, mut escaped) = (false, false);
    let (mut angle, mut comment) = (0usize, 0usize);
    for c in value.chars() {
        if escaped {
            current.push(c);
            escaped = false;
            continue;
        }
        match c {
            '\\' if quoted || comment > 0 => {
                current.push(c);
                escaped = true;
            }
            '"' if comment == 0 => {
                quoted = !quoted;
                current.push(c);
            }
            '(' if !quoted => {
                comment += 1;
                current.push(c);
            }
            ')' if !quoted && comment > 0 => {
                comment -= 1;
                current.push(c);
            }
            '<' if !quoted && comment == 0 => {
                angle += 1;
                current.push(c);
            }
            '>' if !quoted && comment == 0 => {
                angle = angle.saturating_sub(1);
                current.push(c);
            }
            ':' if !quoted && comment == 0 && angle == 0 && !current.contains('@') => {
                // "Group name:" starts a group; its name is not an address.
                current.clear();
            }
            ',' | ';' if !quoted && comment == 0 && angle == 0 => {
                items.push(std::mem::take(&mut current));
            }
            _ => current.push(c),
        }
    }
    items.push(current);
    items
}

fn parse_one(item: &str) -> Option<Contact> {
    let item = item.trim();
    if item.is_empty() {
        return None;
    }
    if let (Some(open), Some(close)) = (item.rfind('<'), item.rfind('>'))
        && open < close
    {
        let address = item[open + 1..close].trim();
        let address = address.rsplit(':').next().unwrap_or(address); // source routes
        if !valid_address(address) {
            return None;
        }
        let name = clean_name(&item[..open]);
        return Some(Contact {
            name,
            address: address.to_string(),
        });
    }
    // No angle brackets: an address, maybe with a (comment) holding the name.
    let (bare, comment) = strip_comments(item);
    let bare = bare.trim().trim_matches('"');
    if valid_address(bare) {
        return Some(Contact {
            name: clean_name(&comment),
            address: bare.to_string(),
        });
    }
    None
}

/// The text outside comments, and the text of the first comment.
fn strip_comments(item: &str) -> (String, String) {
    let mut outside = String::new();
    let mut inside = String::new();
    let mut depth = 0usize;
    for c in item.chars() {
        match c {
            '(' => {
                depth += 1;
                if depth > 1 {
                    inside.push(c);
                }
            }
            ')' if depth > 0 => {
                depth -= 1;
                if depth > 0 {
                    inside.push(c);
                }
            }
            _ if depth > 0 => inside.push(c),
            _ => outside.push(c),
        }
    }
    (outside, inside)
}

fn clean_name(raw: &str) -> String {
    let (outside, _) = strip_comments(raw);
    let trimmed = outside.trim();
    let unquoted = if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        let inner = &trimmed[1..trimmed.len() - 1];
        let mut out = String::with_capacity(inner.len());
        let mut escaped = false;
        for c in inner.chars() {
            if escaped {
                out.push(c);
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else {
                out.push(c);
            }
        }
        out
    } else {
        trimmed.to_string()
    };
    decode_words(&unquoted)
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .to_string()
}

/// A permissive check: one `@`, something on both sides, no spaces or control characters, not absurdly long.
#[must_use]
pub fn valid_address(address: &str) -> bool {
    let Some((local, domain)) = address.rsplit_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.is_empty()
        && address.len() <= 320
        && !address
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | '"' | ','))
}
