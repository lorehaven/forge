//! A small IMAP4rev1 server over in-memory mailboxes, speaking the way Dovecot and Gmail do for the commands a client uses.

use crate::io::{Conn, Stream, acceptor};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

/// How the server behaves.
#[derive(Debug, Clone)]
pub struct ImapOptions {
    /// TLS from the first byte (port 993 style).
    pub implicit_tls: bool,
    /// Offer STARTTLS on a plain connection.
    pub starttls: bool,
    /// Offer `AUTH=PLAIN` (otherwise only `LOGIN`).
    pub auth_plain: bool,
    /// Offer `AUTH=XOAUTH2`.
    pub xoauth2: bool,
    pub move_ext: bool,
    pub uidplus: bool,
    /// Announce `LOGINDISABLED` before TLS, like many servers do.
    pub logindisabled_before_tls: bool,
    /// Hang up after this many commands (0 = never): for failure tests.
    pub drop_after_commands: usize,
    /// Answer the first fetch of a message body with a different, wrong UID (a hostile server).
    pub hostile_fetch: bool,
}

impl Default for ImapOptions {
    fn default() -> Self {
        Self {
            implicit_tls: true,
            starttls: false,
            auth_plain: true,
            xoauth2: false,
            move_ext: true,
            uidplus: true,
            logindisabled_before_tls: false,
            drop_after_commands: 0,
            hostile_fetch: false,
        }
    }
}

/// A message as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMessage {
    pub uid: u32,
    pub flags: Vec<String>,
    pub internal_date: i64,
    pub raw: Vec<u8>,
}

#[derive(Debug, Clone)]
struct FolderData {
    uid_validity: u32,
    uid_next: u32,
    attributes: Vec<String>,
    messages: Vec<StoredMessage>,
}

#[derive(Debug, Default)]
struct AccountData {
    password: String,
    token: Option<String>,
    folders: BTreeMap<String, FolderData>,
}

#[derive(Debug, Default)]
struct State {
    accounts: HashMap<String, AccountData>,
    log: Vec<String>,
    logins: u32,
    failed_logins: u32,
    connections: u32,
}

/// A running server. Dropping it stops accepting.
#[derive(Debug)]
pub struct ImapServer {
    addr: SocketAddr,
    state: Arc<Mutex<State>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for ImapServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl ImapServer {
    /// Starts on a free local port.
    ///
    /// # Panics
    ///
    /// When no port can be bound.
    pub async fn start(options: ImapOptions) -> Self {
        Self::start_on(0, options).await
    }

    /// Starts on `port` (0 for a free one).
    ///
    /// # Panics
    ///
    /// When the port cannot be bound.
    pub async fn start_on(port: u16, options: ImapOptions) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", port)).await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let state = Arc::new(Mutex::new(State::default()));
        let shared = state.clone();
        let task = tokio::spawn(async move {
            loop {
                let Ok((tcp, _)) = listener.accept().await else {
                    return;
                };
                let state = shared.clone();
                let options = options.clone();
                tokio::spawn(async move {
                    let stream = if options.implicit_tls {
                        match acceptor().accept(tcp).await {
                            Ok(tls) => Stream::Tls(Box::new(tls)),
                            Err(_) => return,
                        }
                    } else {
                        Stream::Plain(tcp)
                    };
                    state.lock().unwrap().connections += 1;
                    Session::new(Conn::new(stream), state, options).run().await;
                });
            }
        });
        Self { addr, state, task }
    }

    /// The port it listens on.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// Adds a user with a password.
    pub fn add_account(&self, user: &str, password: &str) {
        let mut state = self.state.lock().unwrap();
        let account = state.accounts.entry(user.to_string()).or_default();
        account.password = password.to_string();
        account
            .folders
            .entry("INBOX".to_string())
            .or_insert_with(|| folder(&[]));
    }

    /// Makes `token` a valid XOAUTH2 token for the user.
    pub fn set_token(&self, user: &str, token: &str) {
        let mut state = self.state.lock().unwrap();
        state.accounts.entry(user.to_string()).or_default().token = Some(token.to_string());
    }

    /// Adds a folder (with SPECIAL-USE attributes such as `\Sent`).
    pub fn add_folder(&self, user: &str, name: &str, attributes: &[&str]) {
        let mut state = self.state.lock().unwrap();
        if let Some(account) = state.accounts.get_mut(user) {
            account
                .folders
                .entry(name.to_string())
                .or_insert_with(|| folder(attributes));
        }
    }

    /// Delivers a message into a folder; returns its UID.
    ///
    /// # Panics
    ///
    /// When the user or folder does not exist.
    pub fn deliver(&self, user: &str, folder_name: &str, raw: &[u8], flags: &[&str]) -> u32 {
        let mut state = self.state.lock().unwrap();
        let folder = state
            .accounts
            .get_mut(user)
            .and_then(|a| a.folders.get_mut(folder_name))
            .expect("user and folder exist");
        let uid = folder.uid_next;
        folder.uid_next += 1;
        folder.messages.push(StoredMessage {
            uid,
            flags: flags.iter().map(ToString::to_string).collect(),
            internal_date: 1_700_000_000 + i64::from(uid) * 60,
            raw: raw.to_vec(),
        });
        uid
    }

    /// What is in a folder now.
    #[must_use]
    pub fn messages(&self, user: &str, folder_name: &str) -> Vec<StoredMessage> {
        let state = self.state.lock().unwrap();
        state
            .accounts
            .get(user)
            .and_then(|a| a.folders.get(folder_name))
            .map(|f| f.messages.clone())
            .unwrap_or_default()
    }

    /// Changes a folder's UIDVALIDITY (the server "lost" its numbering) and empties it.
    pub fn reset_folder(&self, user: &str, folder_name: &str) {
        let mut state = self.state.lock().unwrap();
        if let Some(folder) = state
            .accounts
            .get_mut(user)
            .and_then(|a| a.folders.get_mut(folder_name))
        {
            folder.uid_validity += 1;
            folder.uid_next = 1;
            folder.messages.clear();
        }
    }

    /// Removes a message as another client would.
    pub fn expunge_uid(&self, user: &str, folder_name: &str, uid: u32) {
        let mut state = self.state.lock().unwrap();
        if let Some(folder) = state
            .accounts
            .get_mut(user)
            .and_then(|a| a.folders.get_mut(folder_name))
        {
            folder.messages.retain(|m| m.uid != uid);
        }
    }

    /// Sets a message's flags as another client would.
    pub fn set_flags(&self, user: &str, folder_name: &str, uid: u32, flags: &[&str]) {
        let mut state = self.state.lock().unwrap();
        if let Some(folder) = state
            .accounts
            .get_mut(user)
            .and_then(|a| a.folders.get_mut(folder_name))
            && let Some(message) = folder.messages.iter_mut().find(|m| m.uid == uid)
        {
            message.flags = flags.iter().map(ToString::to_string).collect();
        }
    }

    /// Every command line received, in order (as sent, so passwords are in it: tests check what the client did).
    #[must_use]
    pub fn log(&self) -> Vec<String> {
        self.state.lock().unwrap().log.clone()
    }

    /// Successful logins so far.
    #[must_use]
    pub fn logins(&self) -> u32 {
        self.state.lock().unwrap().logins
    }

    /// Refused logins so far.
    #[must_use]
    pub fn failed_logins(&self) -> u32 {
        self.state.lock().unwrap().failed_logins
    }

    /// Connections accepted so far.
    #[must_use]
    pub fn connections(&self) -> u32 {
        self.state.lock().unwrap().connections
    }
}

fn folder(attributes: &[&str]) -> FolderData {
    FolderData {
        uid_validity: 1_000,
        uid_next: 1,
        attributes: attributes.iter().map(ToString::to_string).collect(),
        messages: Vec::new(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Arg {
    Atom(String),
    Str(Vec<u8>),
    List(Vec<Arg>),
}

impl Arg {
    fn text(&self) -> String {
        match self {
            Self::Atom(a) => a.clone(),
            Self::Str(s) => String::from_utf8_lossy(s).into_owned(),
            Self::List(_) => String::new(),
        }
    }
}

fn tokenize(input: &[u8]) -> Vec<Arg> {
    fn parse(input: &[u8], at: &mut usize, until_paren: bool) -> Vec<Arg> {
        let mut out = Vec::new();
        while *at < input.len() {
            match input[*at] {
                b' ' | b'\r' | b'\n' => *at += 1,
                b')' => {
                    *at += 1;
                    if until_paren {
                        return out;
                    }
                }
                b'(' => {
                    *at += 1;
                    out.push(Arg::List(parse(input, at, true)));
                }
                b'"' => {
                    *at += 1;
                    let mut s = Vec::new();
                    while *at < input.len() && input[*at] != b'"' {
                        if input[*at] == b'\\' {
                            *at += 1;
                        }
                        if *at < input.len() {
                            s.push(input[*at]);
                        }
                        *at += 1;
                    }
                    *at += 1;
                    out.push(Arg::Str(s));
                }
                b'{' => {
                    let close = input[*at..].iter().position(|b| *b == b'}').unwrap_or(0);
                    let n: usize = String::from_utf8_lossy(&input[*at + 1..*at + close])
                        .trim_end_matches('+')
                        .parse()
                        .unwrap_or(0);
                    *at += close + 1;
                    while *at < input.len() && (input[*at] == b'\r' || input[*at] == b'\n') {
                        *at += 1;
                        if input.get(*at - 1) == Some(&b'\n') {
                            break;
                        }
                    }
                    let end = (*at + n).min(input.len());
                    out.push(Arg::Str(input[*at..end].to_vec()));
                    *at = end;
                }
                _ => {
                    let start = *at;
                    let mut bracket = 0;
                    while *at < input.len() {
                        let b = input[*at];
                        if b == b'[' {
                            bracket += 1;
                        } else if b == b']' {
                            bracket -= 1;
                        } else if bracket <= 0 && matches!(b, b' ' | b'(' | b')' | b'\r' | b'\n') {
                            break;
                        }
                        *at += 1;
                    }
                    out.push(Arg::Atom(
                        String::from_utf8_lossy(&input[start..*at]).into_owned(),
                    ));
                }
            }
        }
        out
    }
    let mut at = 0;
    parse(input, &mut at, false)
}

#[derive(PartialEq, Eq)]
enum Phase {
    NotAuthenticated,
    Authenticated,
    Selected,
}

struct Session {
    conn: Conn,
    state: Arc<Mutex<State>>,
    options: ImapOptions,
    phase: Phase,
    user: String,
    folder: String,
    read_only: bool,
    commands: usize,
}

impl Session {
    const fn new(conn: Conn, state: Arc<Mutex<State>>, options: ImapOptions) -> Self {
        Self {
            conn,
            state,
            options,
            phase: Phase::NotAuthenticated,
            user: String::new(),
            folder: String::new(),
            read_only: false,
            commands: 0,
        }
    }

    fn capabilities(&self) -> String {
        let mut caps = vec!["IMAP4rev1", "LITERAL+", "SASL-IR", "SPECIAL-USE"];
        if self.options.uidplus {
            caps.push("UIDPLUS");
        }
        if self.options.move_ext {
            caps.push("MOVE");
        }
        if self.options.starttls && !self.conn.is_tls() {
            caps.push("STARTTLS");
        }
        if self.options.logindisabled_before_tls && !self.conn.is_tls() {
            caps.push("LOGINDISABLED");
        } else if self.options.auth_plain {
            caps.push("AUTH=PLAIN");
        }
        if self.options.xoauth2 && self.conn.is_tls() {
            caps.push("AUTH=XOAUTH2");
        }
        caps.join(" ")
    }

    async fn run(mut self) {
        let greeting = format!(
            "* OK [CAPABILITY {}] mail-testkit ready.\r\n",
            self.capabilities()
        );
        if !self.conn.send(greeting.as_bytes()).await {
            return;
        }
        while let Some(command) = self.read_command().await {
            self.commands += 1;
            if self.options.drop_after_commands > 0
                && self.commands > self.options.drop_after_commands
            {
                return;
            }
            let (tag, rest) = split_tag(&command);
            self.state
                .lock()
                .unwrap()
                .log
                .push(String::from_utf8_lossy(&command).trim_end().to_string());
            if !self.dispatch(&tag, &rest).await {
                return;
            }
        }
    }

    /// One command, with its literals read in.
    async fn read_command(&mut self) -> Option<Vec<u8>> {
        let mut command = self.conn.line().await?;
        loop {
            let trimmed = command
                .strip_suffix(b"\r\n")
                .or_else(|| command.strip_suffix(b"\n"))
                .unwrap_or(&command);
            let Some(n) = literal_len(trimmed) else {
                return Some(command);
            };
            if !trimmed.ends_with(b"+}") && !self.conn.send(b"+ go ahead\r\n").await {
                return None;
            }
            let data = self.conn.exact(n).await?;
            command.extend_from_slice(&data);
            let more = self.conn.line().await?;
            command.extend_from_slice(&more);
        }
    }

    async fn reply(&mut self, text: String) -> bool {
        self.conn.send(text.as_bytes()).await
    }

    async fn dispatch(&mut self, tag: &str, rest: &[u8]) -> bool {
        let args = tokenize(rest);
        let Some(first) = args.first() else {
            return self.reply(format!("{tag} BAD empty command\r\n")).await;
        };
        let verb = first.text().to_ascii_uppercase();
        let args = &args[1..];
        match verb.as_str() {
            "CAPABILITY" => {
                let text = format!("* CAPABILITY {}\r\n{tag} OK done\r\n", self.capabilities());
                self.reply(text).await
            }
            "NOOP" => self.reply(format!("{tag} OK done\r\n")).await,
            "LOGOUT" => {
                let _ = self
                    .reply(format!("* BYE bye\r\n{tag} OK logged out\r\n"))
                    .await;
                false
            }
            "STARTTLS" => {
                if !self.options.starttls || self.conn.is_tls() {
                    return self.reply(format!("{tag} BAD no STARTTLS here\r\n")).await;
                }
                if !self.reply(format!("{tag} OK begin TLS\r\n")).await {
                    return false;
                }
                self.conn.upgrade(&acceptor()).await
            }
            "LOGIN" => self.login(tag, args).await,
            "AUTHENTICATE" => self.authenticate(tag, args).await,
            _ if self.phase == Phase::NotAuthenticated => {
                self.reply(format!("{tag} BAD log in first\r\n")).await
            }
            "LIST" => self.list(tag).await,
            "CREATE" => self.create(tag, args).await,
            "SELECT" | "EXAMINE" => self.select(tag, args, verb == "EXAMINE").await,
            "STATUS" => self.status(tag, args).await,
            "APPEND" => self.append(tag, args).await,
            _ if self.phase != Phase::Selected => {
                self.reply(format!("{tag} BAD select a folder first\r\n"))
                    .await
            }
            "UID" => self.uid(tag, args).await,
            "EXPUNGE" => self.expunge(tag, None).await,
            "CLOSE" => {
                self.phase = Phase::Authenticated;
                self.reply(format!("{tag} OK closed\r\n")).await
            }
            _ => self.reply(format!("{tag} BAD unknown command\r\n")).await,
        }
    }

    async fn login(&mut self, tag: &str, args: &[Arg]) -> bool {
        if self.options.logindisabled_before_tls && !self.conn.is_tls() {
            return self
                .reply(format!("{tag} NO [PRIVACYREQUIRED] login disabled\r\n"))
                .await;
        }
        let user = args.first().map(Arg::text).unwrap_or_default();
        let password = args.get(1).map(Arg::text).unwrap_or_default();
        self.finish_login(tag, &user, &password, false).await
    }

    async fn finish_login(&mut self, tag: &str, user: &str, secret: &str, token: bool) -> bool {
        let ok = {
            let mut state = self.state.lock().unwrap();
            let ok = state.accounts.get(user).is_some_and(|a| {
                if token {
                    a.token.as_deref() == Some(secret)
                } else {
                    a.password == secret && !a.password.is_empty()
                }
            });
            if ok {
                state.logins += 1;
            } else {
                state.failed_logins += 1;
            }
            ok
        };
        if ok {
            self.user = user.to_string();
            self.phase = Phase::Authenticated;
            self.reply(format!(
                "{tag} OK [CAPABILITY {}] logged in\r\n",
                self.capabilities()
            ))
            .await
        } else {
            self.reply(format!(
                "{tag} NO [AUTHENTICATIONFAILED] invalid credentials\r\n"
            ))
            .await
        }
    }

    async fn authenticate(&mut self, tag: &str, args: &[Arg]) -> bool {
        let mechanism = args
            .first()
            .map(|a| a.text().to_ascii_uppercase())
            .unwrap_or_default();
        if !self.conn.is_tls() && !self.options.starttls && !self.options.implicit_tls {
            // Plain-text servers still allow it; tests check the client's refusal, not ours.
        }
        let initial = if let Some(a) = args.get(1) {
            a.text()
        } else {
            if !self.reply("+ \r\n".to_string()).await {
                return false;
            }
            let Some(line) = self.conn.line().await else {
                return false;
            };
            String::from_utf8_lossy(&line).trim().to_string()
        };
        let decoded = decode_base64(&initial);
        match mechanism.as_str() {
            "PLAIN" if self.options.auth_plain => {
                let mut parts = decoded.split(|b| *b == 0);
                let _authzid = parts.next();
                let user = String::from_utf8_lossy(parts.next().unwrap_or_default()).into_owned();
                let password =
                    String::from_utf8_lossy(parts.next().unwrap_or_default()).into_owned();
                self.finish_login(tag, &user, &password, false).await
            }
            "XOAUTH2" if self.options.xoauth2 => {
                let text = String::from_utf8_lossy(&decoded).into_owned();
                let user = text
                    .split('\u{1}')
                    .find_map(|p| p.strip_prefix("user="))
                    .unwrap_or("")
                    .to_string();
                let token = text
                    .split('\u{1}')
                    .find_map(|p| p.strip_prefix("auth=Bearer "))
                    .unwrap_or("")
                    .to_string();
                self.finish_login(tag, &user, &token, true).await
            }
            _ => {
                self.reply(format!("{tag} NO unsupported mechanism\r\n"))
                    .await
            }
        }
    }

    async fn list(&mut self, tag: &str) -> bool {
        let lines: Vec<String> = {
            let state = self.state.lock().unwrap();
            state.accounts[&self.user]
                .folders
                .iter()
                .map(|(name, f)| {
                    let attributes = f.attributes.join(" ");
                    format!("* LIST ({attributes}) \"/\" \"{}\"\r\n", utf7(name))
                })
                .collect()
        };
        let mut out = lines.concat();
        out.push_str(&format!("{tag} OK LIST done\r\n"));
        self.reply(out).await
    }

    async fn create(&mut self, tag: &str, args: &[Arg]) -> bool {
        let name = unutf7(&args.first().map(Arg::text).unwrap_or_default());
        self.state
            .lock()
            .unwrap()
            .accounts
            .get_mut(&self.user)
            .map(|a| a.folders.entry(name).or_insert_with(|| folder(&[])));
        self.reply(format!("{tag} OK created\r\n")).await
    }

    async fn select(&mut self, tag: &str, args: &[Arg], examine: bool) -> bool {
        let name = unutf7(&args.first().map(Arg::text).unwrap_or_default());
        let name = if name.eq_ignore_ascii_case("INBOX") {
            "INBOX".to_string()
        } else {
            name
        };
        let found = {
            let state = self.state.lock().unwrap();
            state.accounts[&self.user]
                .folders
                .get(&name)
                .map(|f| (f.messages.len(), f.uid_validity, f.uid_next))
        };
        let Some((exists, validity, next)) = found else {
            return self
                .reply(format!("{tag} NO [NONEXISTENT] no such folder\r\n"))
                .await;
        };
        self.folder = name;
        self.phase = Phase::Selected;
        self.read_only = examine;
        let mode = if examine { "READ-ONLY" } else { "READ-WRITE" };
        self.reply(format!(
            "* FLAGS (\\Answered \\Flagged \\Deleted \\Seen \\Draft)\r\n* {exists} EXISTS\r\n* 0 RECENT\r\n* OK [UIDVALIDITY {validity}] ok\r\n* OK [UIDNEXT {next}] ok\r\n{tag} OK [{mode}] selected\r\n"
        ))
        .await
    }

    async fn status(&mut self, tag: &str, args: &[Arg]) -> bool {
        let name = unutf7(&args.first().map(Arg::text).unwrap_or_default());
        let found = {
            let state = self.state.lock().unwrap();
            state.accounts[&self.user].folders.get(&name).map(|f| {
                let unseen = f
                    .messages
                    .iter()
                    .filter(|m| !m.flags.iter().any(|x| x.eq_ignore_ascii_case("\\Seen")))
                    .count();
                (f.messages.len(), f.uid_next, f.uid_validity, unseen)
            })
        };
        let Some((messages, next, validity, unseen)) = found else {
            return self.reply(format!("{tag} NO no such folder\r\n")).await;
        };
        self.reply(format!(
            "* STATUS \"{}\" (MESSAGES {messages} UIDNEXT {next} UIDVALIDITY {validity} UNSEEN {unseen})\r\n{tag} OK done\r\n",
            utf7(&name)
        ))
        .await
    }

    async fn append(&mut self, tag: &str, args: &[Arg]) -> bool {
        let name = unutf7(&args.first().map(Arg::text).unwrap_or_default());
        let flags: Vec<String> = args
            .iter()
            .find_map(|a| {
                if let Arg::List(l) = a {
                    Some(l.iter().map(Arg::text).collect())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        let data = args.iter().rev().find_map(|a| {
            if let Arg::Str(s) = a {
                Some(s.clone())
            } else {
                None
            }
        });
        let stored: Result<(u32, u32), &'static str> = {
            let mut state = self.state.lock().unwrap();
            match (
                state
                    .accounts
                    .get_mut(&self.user)
                    .and_then(|a| a.folders.get_mut(&name)),
                data,
            ) {
                (None, _) => Err("NO [TRYCREATE] no such folder"),
                (Some(_), None) => Err("BAD no message"),
                (Some(folder), Some(data)) => {
                    let uid = folder.uid_next;
                    folder.uid_next += 1;
                    folder.messages.push(StoredMessage {
                        uid,
                        flags,
                        internal_date: 1_700_000_000 + i64::from(uid) * 60,
                        raw: data,
                    });
                    Ok((uid, folder.uid_validity))
                }
            }
        };
        match stored {
            Ok((uid, validity)) => {
                self.reply(format!(
                    "{tag} OK [APPENDUID {validity} {uid}] appended\r\n"
                ))
                .await
            }
            Err(text) => self.reply(format!("{tag} {text}\r\n")).await,
        }
    }

    async fn uid(&mut self, tag: &str, args: &[Arg]) -> bool {
        let sub = args
            .first()
            .map(|a| a.text().to_ascii_uppercase())
            .unwrap_or_default();
        let args = &args[1.min(args.len())..];
        match sub.as_str() {
            "SEARCH" => self.search(tag, args).await,
            "FETCH" => self.fetch(tag, args).await,
            "STORE" => self.store(tag, args).await,
            "COPY" => self.copy(tag, args, false).await,
            "MOVE" if self.options.move_ext => self.copy(tag, args, true).await,
            "EXPUNGE" => {
                let set = args.first().map(Arg::text);
                self.expunge(tag, set).await
            }
            _ => {
                self.reply(format!("{tag} BAD unknown UID command\r\n"))
                    .await
            }
        }
    }

    fn with_folder<R>(&self, f: impl FnOnce(&mut FolderData) -> R) -> Option<R> {
        let mut state = self.state.lock().unwrap();
        state
            .accounts
            .get_mut(&self.user)?
            .folders
            .get_mut(&self.folder)
            .map(f)
    }

    async fn search(&mut self, tag: &str, args: &[Arg]) -> bool {
        let args: Vec<Arg> = {
            // Drop "CHARSET UTF-8".
            let mut it = args.to_vec();
            if it
                .first()
                .is_some_and(|a| a.text().eq_ignore_ascii_case("CHARSET"))
            {
                it.drain(..2.min(it.len()));
            }
            it
        };
        let found = self
            .with_folder(|f| {
                let max = f.messages.iter().map(|m| m.uid).max().unwrap_or(0);
                let mut pos = 0;
                let mut out = Vec::new();
                let criteria = parse_search(&args, &mut pos, max);
                for m in &f.messages {
                    if criteria.matches(m) {
                        out.push(m.uid);
                    }
                }
                out
            })
            .unwrap_or_default();
        let list = found
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(" ");
        let line = if list.is_empty() {
            "* SEARCH\r\n".to_string()
        } else {
            format!("* SEARCH {list}\r\n")
        };
        self.reply(format!("{line}{tag} OK search done\r\n")).await
    }

    async fn fetch(&mut self, tag: &str, args: &[Arg]) -> bool {
        let set = args.first().map(Arg::text).unwrap_or_default();
        let items: Vec<Arg> = match args.get(1) {
            Some(Arg::List(l)) => l.clone(),
            Some(other) => vec![other.clone()],
            None => Vec::new(),
        };
        let hostile = self.options.hostile_fetch;
        let (messages, max): (Vec<(usize, StoredMessage)>, u32) = self
            .with_folder(|f| {
                let max = f.messages.iter().map(|m| m.uid).max().unwrap_or(0);
                let ranges = parse_set(&set, max);
                (
                    f.messages
                        .iter()
                        .enumerate()
                        .filter(|(_, m)| in_set(&ranges, m.uid))
                        .map(|(i, m)| (i + 1, m.clone()))
                        .collect(),
                    max,
                )
            })
            .unwrap_or_default();
        let _ = max;
        let mut out: Vec<u8> = Vec::new();
        for (seq, message) in messages {
            let uid = if hostile {
                message.uid + 1000
            } else {
                message.uid
            };
            let mut parts: Vec<Vec<u8>> = vec![format!("UID {uid}").into_bytes()];
            let mut wants_flags = false;
            for item in &items {
                let Arg::Atom(name) = item else { continue };
                let upper = name.to_ascii_uppercase();
                if upper == "FLAGS" {
                    wants_flags = true;
                } else if upper == "INTERNALDATE" {
                    parts.push(
                        format!("INTERNALDATE \"{}\"", imap_date(message.internal_date))
                            .into_bytes(),
                    );
                } else if upper == "RFC822.SIZE" {
                    parts.push(format!("RFC822.SIZE {}", message.raw.len()).into_bytes());
                } else if upper.starts_with("BODY.PEEK[HEADER.FIELDS")
                    || upper.starts_with("BODY[HEADER.FIELDS")
                {
                    // The header names are the next list item: `BODY.PEEK[HEADER.FIELDS (A B)]` was one atom.
                    let wanted: Vec<String> = name
                        .split_once('(')
                        .map(|(_, r)| {
                            r.trim_end_matches(']')
                                .trim_end_matches(')')
                                .split_whitespace()
                                .map(str::to_ascii_lowercase)
                                .collect()
                        })
                        .unwrap_or_default();
                    let header = header_fields(&message.raw, &wanted);
                    let mut part = format!(
                        "BODY[HEADER.FIELDS ({})] {{{}}}\r\n",
                        wanted.join(" ").to_ascii_uppercase(),
                        header.len()
                    )
                    .into_bytes();
                    part.extend_from_slice(&header);
                    parts.push(part);
                } else if upper == "BODY.PEEK[]" || upper == "BODY[]" || upper == "RFC822" {
                    let mut part = format!("BODY[] {{{}}}\r\n", message.raw.len()).into_bytes();
                    part.extend_from_slice(&message.raw);
                    parts.push(part);
                    if upper != "BODY.PEEK[]" {
                        self.with_folder(|f| {
                            if let Some(m) = f.messages.iter_mut().find(|m| m.uid == message.uid)
                                && !m.flags.iter().any(|x| x.eq_ignore_ascii_case("\\Seen"))
                            {
                                m.flags.push("\\Seen".to_string());
                            }
                        });
                        wants_flags = true;
                    }
                }
            }
            if wants_flags {
                let flags = self
                    .with_folder(|f| {
                        f.messages
                            .iter()
                            .find(|m| m.uid == message.uid)
                            .map(|m| m.flags.join(" "))
                    })
                    .flatten()
                    .unwrap_or_default();
                parts.push(format!("FLAGS ({flags})").into_bytes());
            }
            out.extend_from_slice(format!("* {seq} FETCH (").as_bytes());
            for (i, part) in parts.iter().enumerate() {
                if i > 0 {
                    out.push(b' ');
                }
                out.extend_from_slice(part);
            }
            out.extend_from_slice(b")\r\n");
        }
        out.extend_from_slice(format!("{tag} OK fetch done\r\n").as_bytes());
        self.conn.send(&out).await
    }

    async fn store(&mut self, tag: &str, args: &[Arg]) -> bool {
        if self.read_only {
            return self
                .reply(format!("{tag} NO [READ-ONLY] folder is read-only\r\n"))
                .await;
        }
        let set = args.first().map(Arg::text).unwrap_or_default();
        let mode = args
            .get(1)
            .map(|a| a.text().to_ascii_uppercase())
            .unwrap_or_default();
        let flags: Vec<String> = match args.get(2) {
            Some(Arg::List(l)) => l.iter().map(Arg::text).collect(),
            Some(other) => vec![other.text()],
            None => Vec::new(),
        };
        self.with_folder(|f| {
            let max = f.messages.iter().map(|m| m.uid).max().unwrap_or(0);
            let ranges = parse_set(&set, max);
            for m in f.messages.iter_mut().filter(|m| in_set(&ranges, m.uid)) {
                if mode.starts_with('+') {
                    for flag in &flags {
                        if !m.flags.iter().any(|x| x.eq_ignore_ascii_case(flag)) {
                            m.flags.push(flag.clone());
                        }
                    }
                } else if mode.starts_with('-') {
                    m.flags
                        .retain(|x| !flags.iter().any(|f| f.eq_ignore_ascii_case(x)));
                } else {
                    m.flags.clone_from(&flags);
                }
            }
        });
        self.reply(format!("{tag} OK store done\r\n")).await
    }

    async fn copy(&mut self, tag: &str, args: &[Arg], remove: bool) -> bool {
        let set = args.first().map(Arg::text).unwrap_or_default();
        let destination = unutf7(&args.get(1).map(Arg::text).unwrap_or_default());
        let source = self.folder.clone();
        let result = {
            let mut state = self.state.lock().unwrap();
            let Some(account) = state.accounts.get_mut(&self.user) else {
                return false;
            };
            if !account.folders.contains_key(&destination) {
                None
            } else {
                let max = account.folders[&source]
                    .messages
                    .iter()
                    .map(|m| m.uid)
                    .max()
                    .unwrap_or(0);
                let ranges = parse_set(&set, max);
                let chosen: Vec<StoredMessage> = account.folders[&source]
                    .messages
                    .iter()
                    .filter(|m| in_set(&ranges, m.uid))
                    .cloned()
                    .collect();
                let target = account.folders.get_mut(&destination).expect("checked");
                for m in &chosen {
                    let uid = target.uid_next;
                    target.uid_next += 1;
                    target.messages.push(StoredMessage { uid, ..m.clone() });
                }
                if remove {
                    let uids: BTreeSet<u32> = chosen.iter().map(|m| m.uid).collect();
                    account
                        .folders
                        .get_mut(&source)
                        .expect("source")
                        .messages
                        .retain(|m| !uids.contains(&m.uid));
                }
                Some(())
            }
        };
        match result {
            Some(()) => self.reply(format!("{tag} OK done\r\n")).await,
            None => {
                self.reply(format!("{tag} NO [TRYCREATE] no such folder\r\n"))
                    .await
            }
        }
    }

    async fn expunge(&mut self, tag: &str, set: Option<String>) -> bool {
        if self.read_only {
            return self
                .reply(format!("{tag} NO [READ-ONLY] folder is read-only\r\n"))
                .await;
        }
        self.with_folder(|f| {
            let max = f.messages.iter().map(|m| m.uid).max().unwrap_or(0);
            let ranges = set.as_deref().map(|s| parse_set(s, max));
            f.messages.retain(|m| {
                let deleted = m.flags.iter().any(|x| x.eq_ignore_ascii_case("\\Deleted"));
                let selected = ranges.as_ref().is_none_or(|r| in_set(r, m.uid));
                !(deleted && selected)
            });
        });
        self.reply(format!("{tag} OK expunged\r\n")).await
    }
}

fn split_tag(command: &[u8]) -> (String, Vec<u8>) {
    let space = command
        .iter()
        .position(|b| *b == b' ')
        .unwrap_or(command.len());
    (
        String::from_utf8_lossy(&command[..space]).into_owned(),
        command[(space + 1).min(command.len())..].to_vec(),
    )
}

fn literal_len(line: &[u8]) -> Option<usize> {
    let inner = line.strip_suffix(b"}")?;
    let open = inner.iter().rposition(|b| *b == b'{')?;
    let digits = &inner[open + 1..];
    let digits = digits.strip_suffix(b"+").unwrap_or(digits);
    std::str::from_utf8(digits).ok()?.parse().ok()
}

fn decode_base64(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for b in text.bytes() {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xFF) as u8);
        }
    }
    out
}

/// `1:5,7,9:*` -> ranges (`*` is the largest UID).
pub(crate) fn parse_set(set: &str, max: u32) -> Vec<(u32, u32)> {
    set.split(',')
        .filter_map(|part| {
            let num = |s: &str| if s == "*" { Some(max) } else { s.parse().ok() };
            match part.split_once(':') {
                Some((a, b)) => {
                    let (a, b) = (num(a)?, num(b)?);
                    Some((a.min(b), a.max(b)))
                }
                None => num(part).map(|n| (n, n)),
            }
        })
        .collect()
}

fn in_set(ranges: &[(u32, u32)], uid: u32) -> bool {
    ranges.iter().any(|(a, b)| uid >= *a && uid <= *b)
}

/// The header lines for the fields wanted (case-insensitive), with continuation lines, ending in a blank line.
fn header_fields(raw: &[u8], wanted: &[String]) -> Vec<u8> {
    let text = String::from_utf8_lossy(raw);
    let head = text.split("\r\n\r\n").next().unwrap_or("");
    let head = head.split("\n\n").next().unwrap_or(head);
    let mut out = String::new();
    let mut keep = false;
    for line in head.lines() {
        if line.starts_with([' ', '\t']) {
            if keep {
                out.push_str(line);
                out.push_str("\r\n");
            }
            continue;
        }
        let name = line.split(':').next().unwrap_or("").to_ascii_lowercase();
        keep = wanted.contains(&name);
        if keep {
            out.push_str(line);
            out.push_str("\r\n");
        }
    }
    out.push_str("\r\n");
    out.into_bytes()
}

enum Criteria {
    All,
    Unseen,
    Seen,
    Flagged,
    Uid(Vec<(u32, u32)>),
    Header(&'static str, String),
    Text(String),
    Or(Box<Criteria>, Box<Criteria>),
    And(Vec<Criteria>),
}

impl Criteria {
    fn matches(&self, m: &StoredMessage) -> bool {
        let has = |f: &str| m.flags.iter().any(|x| x.eq_ignore_ascii_case(f));
        match self {
            Self::All => true,
            Self::Unseen => !has("\\Seen"),
            Self::Seen => has("\\Seen"),
            Self::Flagged => has("\\Flagged"),
            Self::Uid(r) => in_set(r, m.uid),
            Self::Header(name, needle) => {
                let head = header_fields(&m.raw, &[(*name).to_string()]);
                String::from_utf8_lossy(&head)
                    .to_ascii_lowercase()
                    .contains(&needle.to_ascii_lowercase())
            }
            Self::Text(needle) => String::from_utf8_lossy(&m.raw)
                .to_ascii_lowercase()
                .contains(&needle.to_ascii_lowercase()),
            Self::Or(a, b) => a.matches(m) || b.matches(m),
            Self::And(all) => all.iter().all(|c| c.matches(m)),
        }
    }
}

fn parse_search(args: &[Arg], pos: &mut usize, max: u32) -> Criteria {
    let mut all = Vec::new();
    while *pos < args.len() {
        let word = args[*pos].text().to_ascii_uppercase();
        *pos += 1;
        let next_text = |pos: &mut usize| {
            let t = args.get(*pos).map(Arg::text).unwrap_or_default();
            *pos += 1;
            t
        };
        let criterion = match word.as_str() {
            "ALL" => Criteria::All,
            "UNSEEN" => Criteria::Unseen,
            "SEEN" => Criteria::Seen,
            "FLAGGED" => Criteria::Flagged,
            "UID" => Criteria::Uid(parse_set(&next_text(pos), max)),
            "SUBJECT" => Criteria::Header("subject", next_text(pos)),
            "FROM" => Criteria::Header("from", next_text(pos)),
            "TO" => Criteria::Header("to", next_text(pos)),
            "BODY" | "TEXT" => Criteria::Text(next_text(pos)),
            "OR" => {
                let a = parse_one(args, pos, max);
                let b = parse_one(args, pos, max);
                Criteria::Or(Box::new(a), Box::new(b))
            }
            _ => Criteria::All,
        };
        all.push(criterion);
    }
    Criteria::And(all)
}

fn parse_one(args: &[Arg], pos: &mut usize, max: u32) -> Criteria {
    let end = (*pos + 1).min(args.len());
    let word = args
        .get(*pos)
        .map(|a| a.text().to_ascii_uppercase())
        .unwrap_or_default();
    let take = match word.as_str() {
        "OR" => {
            *pos += 1;
            let a = parse_one(args, pos, max);
            let b = parse_one(args, pos, max);
            return Criteria::Or(Box::new(a), Box::new(b));
        }
        "SUBJECT" | "FROM" | "TO" | "BODY" | "TEXT" | "UID" => 2,
        _ => 1,
    };
    let slice_end = (*pos + take).min(args.len());
    let slice = args[*pos..slice_end.max(end)].to_vec();
    *pos = slice_end.max(end);
    let mut p = 0;
    parse_search(&slice, &mut p, max)
}

fn imap_date(seconds: i64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let days = seconds.div_euclid(86_400);
    let rem = seconds.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!(
        "{day:02}-{}-{year} {:02}:{:02}:{:02} +0000",
        MONTHS[usize::try_from(month - 1).unwrap_or(0)],
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Folder names go over the wire in modified UTF-7.
fn utf7(name: &str) -> String {
    let mut out = String::new();
    let mut pending: Vec<u16> = Vec::new();
    let flush = |pending: &mut Vec<u16>, out: &mut String| {
        if pending.is_empty() {
            return;
        }
        let bytes: Vec<u8> = pending.iter().flat_map(|u| u.to_be_bytes()).collect();
        out.push('&');
        out.push_str(&b64(&bytes).trim_end_matches('=').replace('/', ","));
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
            let mut buf = [0u16; 2];
            pending.extend_from_slice(c.encode_utf16(&mut buf));
        }
    }
    flush(&mut pending, &mut out);
    out
}

fn unutf7(name: &str) -> String {
    let mut out = String::new();
    let mut rest = name;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('-') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let enc = &after[..end];
        if enc.is_empty() {
            out.push('&');
        } else {
            let bytes = decode_base64(&enc.replace(',', "/"));
            let units: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect();
            out.push_str(&String::from_utf16_lossy(&units));
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

fn b64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(char::from(A[(n >> 18) as usize & 63]));
        out.push(char::from(A[(n >> 12) as usize & 63]));
        out.push(if chunk.len() > 1 {
            char::from(A[(n >> 6) as usize & 63])
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            char::from(A[n as usize & 63])
        } else {
            '='
        });
    }
    out
}
