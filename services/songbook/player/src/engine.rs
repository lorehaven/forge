//! The words between the screens and the audio engine (`engine.js`).
//!
//! The screens draw `#sb-engine` with the commands in `data-cmd` (a JSON array of `{n, op, ...}`; the engine does the ones
//! whose `n` it has not seen) and `#sb-engine-in`, an input whose `input` events carry the engine's answers as JSON.

use quench_serde::{Value, json};

/// How the queue repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

impl Repeat {
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::All => "all",
            Self::One => "one",
        }
    }

    fn parse(word: &str) -> Self {
        match word {
            "all" => Self::All,
            "one" => Self::One,
            _ => Self::Off,
        }
    }

    /// The next in the cycle of the repeat button.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Off => Self::All,
            Self::All => Self::One,
            Self::One => Self::Off,
        }
    }
}

/// What is playing, as the engine last said.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct State {
    pub playing: bool,
    /// The song (0: none).
    pub id: i64,
    pub index: i64,
    pub len: usize,
    pub pos: f64,
    pub dur: f64,
    pub shuffle: bool,
    pub repeat: Repeat,
    /// The crossfade in seconds (0: none).
    pub fade: f64,
    pub volume: f64,
    pub loading: bool,
}

/// The songs from the current one on.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Queue {
    pub ids: Vec<i64>,
    pub index: i64,
    pub len: usize,
}

/// What the engine says.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    State(State),
    Queue(Queue),
    /// The songs played last, newest first.
    Recent(Vec<i64>),
    /// The songs kept on this device.
    Cached(Vec<i64>),
    /// Keeping songs: how many are done of how many.
    Saving {
        done: u32,
        total: u32,
    },
    /// What the device has used of what the browser lets the page keep.
    Storage {
        used: u64,
        quota: u64,
    },
    Error(String),
}

fn num(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn int(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(Value::as_i64).unwrap_or(0)
}

fn flag(v: &Value, key: &str) -> bool {
    matches!(v.get(key), Some(Value::Bool(true)))
}

fn ids(v: &Value, key: &str) -> Vec<i64> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default()
}

impl Event {
    /// The event a JSON text says; `None` for anything else.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let v: Value = quench_serde::json::from_str(text).ok()?;
        Some(match v.get("ev")?.as_str()? {
            "state" => Self::State(State {
                playing: flag(&v, "playing"),
                id: int(&v, "id"),
                index: int(&v, "index"),
                len: usize::try_from(int(&v, "len")).unwrap_or(0),
                pos: num(&v, "pos"),
                dur: num(&v, "dur"),
                shuffle: flag(&v, "shuffle"),
                repeat: Repeat::parse(v.get("repeat").and_then(Value::as_str).unwrap_or("")),
                fade: num(&v, "fade"),
                volume: if v.get("vol").is_some() {
                    num(&v, "vol")
                } else {
                    1.0
                },
                loading: flag(&v, "loading"),
            }),
            "queue" => Self::Queue(Queue {
                ids: ids(&v, "ids"),
                index: int(&v, "index"),
                len: usize::try_from(int(&v, "len")).unwrap_or(0),
            }),
            "recent" => Self::Recent(ids(&v, "ids")),
            "cached" => Self::Cached(ids(&v, "ids")),
            "saving" => Self::Saving {
                done: u32::try_from(int(&v, "done")).unwrap_or(0),
                total: u32::try_from(int(&v, "total")).unwrap_or(0),
            },
            "storage" => Self::Storage {
                used: u64::try_from(int(&v, "used")).unwrap_or(0),
                quota: u64::try_from(int(&v, "quota")).unwrap_or(0),
            },
            "error" => Self::Error(
                v.get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            ),
            _ => return None,
        })
    }
}

/// What the screens ask of the engine.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Plays `queue` from `index` (`-1`: from the start, or at random when `shuffle`).
    Load {
        queue: Vec<i64>,
        index: i64,
        shuffle: bool,
    },
    Play,
    Pause,
    Toggle,
    Next,
    Prev,
    Jump(i64),
    Seek(f64),
    Shuffle(bool),
    Repeat(Repeat),
    Fade(f64),
    Volume(f64),
    /// Title, artist and album of songs, for the lock screen.
    Meta(Vec<(i64, String, String, String)>),
    Enqueue {
        ids: Vec<i64>,
        next: bool,
    },
    Remove(i64),
    Keep(Vec<i64>),
    Drop(Vec<i64>),
    /// Say everything again (the screens just came up).
    Ask,
}

impl Command {
    #[must_use]
    pub fn to_value(&self, n: u64) -> Value {
        match self {
            Self::Load {
                queue,
                index,
                shuffle,
            } => {
                json!({ "n": n, "op": "load", "queue": queue, "index": index, "shuffle": shuffle })
            }
            Self::Play => json!({ "n": n, "op": "play" }),
            Self::Pause => json!({ "n": n, "op": "pause" }),
            Self::Toggle => json!({ "n": n, "op": "toggle" }),
            Self::Next => json!({ "n": n, "op": "next" }),
            Self::Prev => json!({ "n": n, "op": "prev" }),
            Self::Jump(i) => json!({ "n": n, "op": "jump", "index": i }),
            Self::Seek(to) => json!({ "n": n, "op": "seek", "to": to }),
            Self::Shuffle(on) => json!({ "n": n, "op": "shuffle", "on": on }),
            Self::Repeat(mode) => json!({ "n": n, "op": "repeat", "mode": mode.word() }),
            Self::Fade(secs) => json!({ "n": n, "op": "fade", "secs": secs }),
            Self::Volume(v) => json!({ "n": n, "op": "volume", "v": v }),
            Self::Meta(songs) => {
                let songs: Vec<Value> = songs
                    .iter()
                    .map(|(id, title, artist, album)| {
                        json!({ "id": id, "title": title, "artist": artist, "album": album })
                    })
                    .collect();
                json!({ "n": n, "op": "meta", "songs": songs })
            }
            Self::Enqueue { ids, next } => {
                json!({ "n": n, "op": "enqueue", "ids": ids, "next": next })
            }
            Self::Remove(i) => json!({ "n": n, "op": "remove", "index": i }),
            Self::Keep(ids) => json!({ "n": n, "op": "keep", "ids": ids }),
            Self::Drop(ids) => json!({ "n": n, "op": "drop", "ids": ids }),
            Self::Ask => json!({ "n": n, "op": "ask" }),
        }
    }
}

/// The commands the engine has not necessarily seen yet: the last few, numbered.
///
/// The attribute carries all of them, because two commands set in one turn of the page would otherwise leave the engine
/// only the second.
#[derive(Debug, Default)]
pub struct Outbox {
    n: u64,
    sent: Vec<Value>,
}

/// How many commands are kept in the attribute.
const KEPT: usize = 24;

impl Outbox {
    /// The last command added, as JSON text (what a host that is not a page is sent).
    #[must_use]
    pub fn last(&self) -> Option<String> {
        self.sent.last().map(quench_serde::json::to_string)
    }

    /// Adds a command; returns the attribute's new text.
    pub fn push(&mut self, command: &Command) -> String {
        self.n += 1;
        self.sent.push(command.to_value(self.n));
        if self.sent.len() > KEPT {
            self.sent.remove(0);
        }
        quench_serde::json::to_string(&Value::Array(self.sent.clone()))
    }
}
