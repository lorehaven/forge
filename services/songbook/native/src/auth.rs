//! Who is signed in to which server, and the access token the calls carry.
//!
//! The password goes once, to Gatehouse, which answers a refresh token for this app (one session per user and app, kept for
//! months); that token is kept sealed ([`crate::store`]) and swapped for short access tokens as they run out, by the same
//! `gatehouse-client` the other apps use. A server that cannot be reached never ends the session: only Gatehouse refusing the
//! refresh token does. A server that answers the library with no sign-in at all (a development one) is an *open* session: no
//! token is sent.

use std::sync::{Mutex, PoisonError};

use crate::store::{Settings, Store};
use gatehouse_client::{Error, Session, client, login_as};

/// The name Gatehouse keeps this app's session under.
pub const CLIENT: &str = "songbook-android";

/// Why something could not be done, in the terms the shell acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// Nobody is signed in, or the session has ended: ask for the password again.
    SignedOut,
    /// The server could not be reached (or answered something that is not an answer): try again later.
    Unreachable(String),
    /// A refusal, in words for the person.
    Refused(String),
}

impl Failure {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::SignedOut => "signed_out",
            Self::Unreachable(_) => "unreachable",
            Self::Refused(_) => "refused",
        }
    }

    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::SignedOut => "signed out".to_string(),
            Self::Unreachable(why) | Self::Refused(why) => why.clone(),
        }
    }
}

/// What a sign-in that was refused means, for a person.
fn refusal(e: &Error) -> Failure {
    match e {
        Error::Network(why) => Failure::Unreachable(why.clone()),
        Error::Unauthorized => Failure::Refused("The user name or password is wrong.".into()),
        Error::AccountLocked => Failure::Refused(
            "This account is locked for a while after too many wrong passwords.".into(),
        ),
        Error::AccountDisabled => Failure::Refused("This account is turned off.".into()),
        Error::EmailUnverified => {
            Failure::Refused("The email address of this account is not confirmed.".into())
        }
        Error::MfaRequired => Failure::Refused(
            "This account has a second factor, which this app cannot ask for.".into(),
        ),
        Error::Status(code) => Failure::Refused(format!("The server answered {code}.")),
    }
}

pub struct Auth {
    store: Store,
    settings: Mutex<Settings>,
    session: Mutex<Option<Session>>,
}

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Auth { .. }")
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `host`, `host:8080` or `https://host/` as `https://host`.
#[must_use]
pub fn normalise(address: &str) -> String {
    let text = address.trim().trim_end_matches('/');
    if text.starts_with("http://") || text.starts_with("https://") {
        text.to_string()
    } else {
        format!("https://{text}")
    }
}

impl Auth {
    /// With the session an earlier run left, if there is one.
    #[must_use]
    pub fn new(store: Store) -> Self {
        let settings = store.settings();
        let session = (!settings.open && !settings.server.is_empty())
            .then(|| store.token())
            .flatten()
            .map(|token| Self::resumed(&store, &settings.server, &token));
        Self {
            store,
            settings: Mutex::new(settings),
            session: Mutex::new(session),
        }
    }

    /// A session that keeps every new refresh token as soon as it is given (a refresh spends the old one).
    fn resumed(store: &Store, server: &str, token: &str) -> Session {
        Self::kept(store, Session::resume(server, token))
    }

    /// Keeps every new refresh token, and follows the one another program of the app (the player's screens and the playback
    /// service each run one) kept after this one: a refresh spends the token it is made with.
    fn kept(store: &Store, session: Session) -> Session {
        let (keep, follow) = (store.clone(), store.clone());
        session
            .keeping_tokens(move |t| keep.set_token(t))
            .following(move || follow.token())
    }

    /// Takes up what another program of the app did: the playback service and the screens each run one of these over the same
    /// files, and a sign-in or a sign-out in one is the other's too.
    #[allow(clippy::significant_drop_tightening)] // both locks are held while the two change together
    fn sync(&self) {
        let kept = self.store.settings();
        let mut mine = lock(&self.settings);
        if *mine == kept {
            return;
        }
        let session = (!kept.open && !kept.server.is_empty())
            .then(|| self.store.token())
            .flatten()
            .map(|token| Self::resumed(&self.store, &kept.server, &token));
        *mine = kept;
        *lock(&self.session) = session;
    }

    #[must_use]
    pub fn settings(&self) -> Settings {
        self.sync();
        lock(&self.settings).clone()
    }

    #[must_use]
    pub fn signed_in(&self) -> bool {
        self.sync();
        let s = lock(&self.settings);
        !s.server.is_empty() && (s.open || lock(&self.session).is_some())
    }

    /// Where the library's API is.
    #[must_use]
    pub fn api(&self) -> String {
        format!("{}/songbook/api/v1", lock(&self.settings).server)
    }

    /// Signs in. An empty `user` asks whether the server needs no sign-in at all.
    ///
    /// # Errors
    /// [`Failure::Refused`] says why in words; [`Failure::Unreachable`] when the server is not there.
    pub fn login(&self, address: &str, user: &str, password: &str) -> Result<(), Failure> {
        let base = normalise(address);
        if user.trim().is_empty() {
            let url = format!("{base}/songbook/api/v1/library/overview");
            let answer = client()
                .get(&url)
                .send()
                .map_err(|e| Failure::Unreachable(e.to_string()))?;
            if answer.status().as_u16() != 200 {
                return Err(Failure::Refused("Enter your user name.".into()));
            }
            self.store.forget();
            let settings = Settings {
                server: base,
                user: String::new(),
                open: true,
            };
            self.store.set_settings(&settings);
            *lock(&self.settings) = settings;
            *lock(&self.session) = None;
            return Ok(());
        }
        let session = login_as(&base, user.trim(), password, CLIENT).map_err(|e| refusal(&e))?;
        self.store.set_token(session.refresh_token());
        let settings = Settings {
            server: base,
            user: user.trim().to_string(),
            open: false,
        };
        self.store.set_settings(&settings);
        *lock(&self.settings) = settings;
        *lock(&self.session) = Some(Self::kept(&self.store, session));
        Ok(())
    }

    pub fn sign_out(&self) {
        self.store.forget();
        *lock(&self.settings) = Settings::default();
        *lock(&self.session) = None;
    }

    /// An access token with life left (renewed when it is about to run out); `None` for an open session.
    ///
    /// # Errors
    /// [`Failure::SignedOut`] when nobody is signed in or Gatehouse refuses the refresh token (which ends the session);
    /// [`Failure::Unreachable`] when it cannot be asked.
    #[allow(clippy::significant_drop_tightening)] // the lock is held for the whole renewal, on purpose: one refresh at a time
    pub fn token(&self) -> Result<Option<String>, Failure> {
        self.sync();
        let open = lock(&self.settings).open;
        if open {
            return Ok(None);
        }
        let mut guard = lock(&self.session);
        let Some(session) = guard.as_mut() else {
            return Err(Failure::SignedOut);
        };
        let fresh = session.fresh_token();
        match fresh {
            Ok(token) => Ok(Some(token)),
            Err(e) => self.refused(&mut guard, &e),
        }
    }

    /// Renews the token now (the server refused the one that was used).
    #[allow(clippy::significant_drop_tightening)] // as `token`
    fn renew(&self) -> Result<(), Failure> {
        let mut guard = lock(&self.session);
        let Some(session) = guard.as_mut() else {
            return Err(Failure::SignedOut);
        };
        let renewed = session.refresh();
        match renewed {
            Ok(()) => Ok(()),
            Err(e) => self.refused(&mut guard, &e).map(|_: Option<String>| ()),
        }
    }

    fn refused<T>(&self, guard: &mut Option<Session>, e: &Error) -> Result<T, Failure> {
        match e {
            // Gatehouse says the token is no good: that, and only that, ends the session.
            Error::Unauthorized | Error::AccountDisabled | Error::AccountLocked => {
                self.store.forget();
                *lock(&self.settings) = Settings::default();
                *guard = None;
                Err(Failure::SignedOut)
            }
            other => Err(Failure::Unreachable(other.to_string())),
        }
    }

    /// A `GET` of `path` under the API, with the token; a refused token is renewed once. The answer's text.
    ///
    /// # Errors
    /// [`Failure`].
    pub fn get(&self, path: &str) -> Result<String, Failure> {
        self.call("GET", path, None)
    }

    /// A `POST` of `body` (JSON) to `path` under the API.
    ///
    /// # Errors
    /// [`Failure`].
    pub fn post(&self, path: &str, body: &str) -> Result<String, Failure> {
        self.call("POST", path, Some(body))
    }

    fn call(&self, method: &str, path: &str, body: Option<&str>) -> Result<String, Failure> {
        let bytes = self.request(method, &format!("{}{path}", self.api()), body)?;
        String::from_utf8(bytes).map_err(|e| Failure::Unreachable(e.to_string()))
    }

    /// A `GET` of `path` on the server itself (a picture's address, `/songbook/api/v1/art/...`), with the token. The bytes.
    ///
    /// # Errors
    /// [`Failure`].
    pub fn fetch(&self, path: &str) -> Result<Vec<u8>, Failure> {
        self.request("GET", &format!("{}{path}", self.settings().server), None)
    }

    fn request(&self, method: &str, url: &str, body: Option<&str>) -> Result<Vec<u8>, Failure> {
        let (status, bytes) = self.exchange(method, url, body)?;
        match status {
            200..=299 => Ok(bytes),
            401 => Err(Failure::SignedOut),
            403 => Err(Failure::Refused(
                "This account may not use Songbook.".into(),
            )),
            other => Err(Failure::Unreachable(format!(
                "The server answered {other}."
            ))),
        }
    }

    /// One call with the token (a refused token is renewed once and the call made again): the status and the body, whatever
    /// the status. A failure is only that the call could not be made.
    ///
    /// # Errors
    /// [`Failure::SignedOut`] when the token cannot be renewed; [`Failure::Unreachable`].
    pub fn exchange(
        &self,
        method: &str,
        url: &str,
        body: Option<&str>,
    ) -> Result<(u16, Vec<u8>), Failure> {
        let mut renewed = false;
        loop {
            let token = self.token()?;
            let mut request = match method {
                "POST" => client().post(url),
                "PUT" => client().put(url),
                "DELETE" => client().delete(url),
                _ => client().get(url),
            };
            if let Some(token) = &token {
                request = request.bearer_auth(token);
            }
            if let Some(body) = body {
                request = request
                    .header("Content-Type", "application/json")
                    .body(body.to_string());
            }
            let response = request
                .send()
                .map_err(|e| Failure::Unreachable(e.to_string()))?;
            let status = response.status().as_u16();
            if status == 401 && !renewed && token.is_some() {
                renewed = true;
                self.renew()?;
                continue;
            }
            let bytes = response
                .bytes()
                .map(|b| b.to_vec())
                .map_err(|e| Failure::Unreachable(e.to_string()))?;
            return Ok((status, bytes));
        }
    }

    /// [`Auth::exchange`] for a path under the API (`/library/home`).
    ///
    /// # Errors
    /// As `exchange`.
    pub fn call_api(
        &self,
        method: &str,
        path: &str,
        body: Option<&str>,
    ) -> Result<(u16, String), Failure> {
        let (status, bytes) = self.exchange(method, &format!("{}{path}", self.api()), body)?;
        Ok((status, String::from_utf8_lossy(&bytes).into_owned()))
    }
}
