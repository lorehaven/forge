//! How songs are told apart and named.
//!
//! A song is identified by its [`song_key`]: the sorted, normalised artist names and the normalised title with
//! `(feat. X)` and remaster tags removed. The same track on an album, a deluxe edition, a compilation or a collaborator's
//! discography has one key, so it is one row in the index and one file on disk. The file is called
//! `<artists in alphabetical order> - <title>` ([`make_filename`]).

/// Lowercase letters and digits of any script, nothing else: `Pięć Dwa Dębiec` -> `pięćdwadębiec`.
#[must_use]
pub fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Splits `s` into bracket groups and the text between them; `keep` decides about each group (with its brackets).
fn without_groups(s: &str, drop: impl Fn(&str) -> bool) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if matches!(chars[i], '(' | '[')
            && let Some(len) = chars[i + 1..].iter().position(|&c| c == ')' || c == ']')
        {
            let end = i + 1 + len;
            let group: String = chars[i..=end].iter().collect();
            if drop(&group) {
                // The whitespace before the group goes with it.
                while out.ends_with(char::is_whitespace) {
                    out.pop();
                }
                i = end + 1;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn is_feat(group: &str) -> bool {
    let inner = group[1..].trim_start().to_lowercase();
    ["featuring", "feat", "ft"].iter().any(|w| {
        inner
            .strip_prefix(w)
            .is_some_and(|rest| !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
    })
}

/// A trailing ` - 2011 Remaster`, ` - Remastered 2011`, ` - Remastered Version`.
fn strip_remaster_suffix(s: &str) -> String {
    if let Some(at) = s.rfind('-') {
        let tail = s[at + 1..].trim().to_lowercase();
        let words: Vec<&str> = tail.split_whitespace().collect();
        let is_year = |w: &str| w.len() == 4 && w.chars().all(|c| c.is_ascii_digit());
        let ok = match words.as_slice() {
            [w] => *w == "remaster" || *w == "remastered",
            [a, b] => {
                (is_year(a) && (*b == "remaster" || *b == "remastered"))
                    || ((*a == "remaster" || *a == "remastered") && (is_year(b) || *b == "version"))
            }
            [a, b, c] => {
                (is_year(a) && (*b == "remaster" || *b == "remastered") && *c == "version")
                    || ((*a == "remaster" || *a == "remastered") && is_year(b) && *c == "version")
            }
            _ => false,
        };
        if ok {
            return s[..at].trim_end().to_string();
        }
    }
    s.to_string()
}

/// The title without `(feat. X)` (the artists are in the file name) and without remaster tags. `Sped Up`, `Live` and
/// `Remix` stay: those are other songs.
#[must_use]
pub fn clean_title(title: &str) -> String {
    let t = without_groups(title, |g| {
        is_feat(g) || g.to_lowercase().contains("remaster")
    });
    let t = strip_remaster_suffix(t.trim());
    let t = t.trim();
    if t.is_empty() {
        let original = title.trim();
        if original.is_empty() {
            "Unknown Title".to_string()
        } else {
            original.to_string()
        }
    } else {
        t.to_string()
    }
}

/// An album title without edition noise: `Illmatic (Deluxe Edition)` and `Illmatic` are one album.
#[must_use]
pub fn album_key(title: &str) -> String {
    const WORDS: [&str; 8] = [
        "deluxe", "edition", "version", "remaster", "expanded", "bonus", "explicit", "clean",
    ];
    norm(&without_groups(title, |g| {
        let g = g.to_lowercase();
        WORDS.iter().any(|w| g.contains(w))
    }))
}

/// The identity of a song: `a|b||title` with the artist names and the title normalised.
#[must_use]
pub fn song_key(names: &[&str], title: &str) -> String {
    let mut people: Vec<String> = names.iter().map(|n| norm(n)).collect();
    people.sort();
    people.dedup();
    format!(
        "{}||{}",
        people.join("|"),
        norm(&clean_title(title).replace('&', " and "))
    )
}

/// The names in alphabetical order (case-insensitive), without a second spelling of the same artist.
#[must_use]
pub fn sorted_names<'a>(names: &[&'a str]) -> Vec<&'a str> {
    let mut sorted: Vec<&str> = names.to_vec();
    sorted.sort_by(|a, b| {
        a.to_lowercase()
            .cmp(&b.to_lowercase())
            .then_with(|| a.cmp(b))
    });
    let mut seen = Vec::new();
    sorted.retain(|n| {
        let k = norm(n);
        if seen.contains(&k) {
            false
        } else {
            seen.push(k);
            true
        }
    });
    sorted
}

/// One name made safe as a file name part: no path separators or characters Windows refuses, single spaces, no trailing
/// dots or spaces.
#[must_use]
pub fn safe_path_part(value: &str) -> String {
    let replaced: String = value
        .chars()
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c < ' ' {
                '_'
            } else {
                c
            }
        })
        .collect();
    let collapsed = replaced.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim_end_matches(['.', ' ']);
    if trimmed.is_empty() {
        "Unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

/// File names stay below this many bytes; filesystems allow 255 including the extension.
pub const MAX_NAME: usize = 200;

fn truncate_bytes(s: &mut String, max: usize) {
    while s.len() > max {
        s.pop();
    }
}

/// `Artist A, Artist B - Title`: artists in alphabetical order, safe for any filesystem, short enough, and never starting
/// with a dot (such files are hidden).
#[must_use]
pub fn make_filename(names: &[&str], title: &str) -> String {
    let joined = sorted_names(names).join(", ");
    let mut artists = safe_path_part(if joined.is_empty() {
        "Unknown Artist"
    } else {
        &joined
    });
    let title = safe_path_part(&clean_title(title));
    let room = MAX_NAME.saturating_sub(title.len() + 3).max(40);
    truncate_bytes(&mut artists, room);
    let artists = artists.trim_end_matches([',', ' ']);
    let mut name = format!("{artists} - {title}");
    truncate_bytes(&mut name, MAX_NAME);
    name.trim_start_matches(['.', ' '])
        .trim_end_matches(['.', ' '])
        .to_string()
}

/// ` [xxxxxx]`: appended to the file name of a song when another song already has the name.
#[must_use]
pub fn disambiguate(name: &str, key: &str) -> String {
    // FNV-1a over the key: stable, no dependencies.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in key.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    let tag = format!(" [{:06x}]", hash & 0x00ff_ffff);
    if name.len() + tag.len() <= MAX_NAME {
        return format!("{}{tag}", name.trim_end_matches(['.', ' ']));
    }
    // Make room by shortening the artists, not the title the file is known by.
    let (artists, title) = name.rsplit_once(" - ").unwrap_or((name, ""));
    let mut artists = artists.to_string();
    let tail = if title.is_empty() {
        String::new()
    } else {
        format!(" - {title}")
    };
    truncate_bytes(
        &mut artists,
        MAX_NAME.saturating_sub(tail.len() + tag.len()),
    );
    let mut out = format!("{}{tail}", artists.trim_end_matches([',', ' ']));
    truncate_bytes(&mut out, MAX_NAME - tag.len());
    format!("{}{tag}", out.trim_end_matches(['.', ' ']))
}
