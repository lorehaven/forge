//! The notices a redirect carries back to a page (`?err=...`, `?ok=...`, `?registered=1`).
//!
//! The service answers every form post with a redirect, and the page shows what the query says. Only keys from a fixed list are
//! ever shown, so a hand-made link cannot put its own text on a page.

/// How a notice reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Ok,
    Error,
}

/// A message (a translation key) and its tone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Notice {
    pub key: &'static str,
    pub tone: Tone,
}

impl Notice {
    const fn error(key: &'static str) -> Self {
        Self {
            key,
            tone: Tone::Error,
        }
    }

    const fn ok(key: &'static str) -> Self {
        Self {
            key,
            tone: Tone::Ok,
        }
    }
}

/// The query of the page's address.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query(Vec<(String, String)>);

impl Query {
    #[must_use]
    pub const fn new(pairs: Vec<(String, String)>) -> Self {
        Self(pairs)
    }

    /// The first value of `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// Whether `name` is there, with any value.
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.get(name).is_some()
    }
}

fn known(candidate: Option<&str>, list: &[&'static str]) -> Option<&'static str> {
    let candidate = candidate?;
    list.iter().find(|key| **key == candidate).copied()
}

/// Errors the sign-in page can be sent back with (besides `err=1`: wrong credentials).
pub const LOGIN_ERRORS: [&str; 8] = [
    "ui_login_verify_invalid",
    "ui_login_reset_invalid",
    "ui_login_account_disabled",
    "ui_login_account_locked",
    "ui_login_email_unverified",
    "ui_login_rate_limited",
    "ui_login_invite_invalid",
    "ui_login_confirm_email_invalid",
];

/// What the sign-in page says about where the person comes from: wrong credentials, a refused account, or good news
/// (a successful reset...). A successful reset is more specific than "link sent", so it comes first.
#[must_use]
pub fn login(query: &Query) -> Option<Notice> {
    if query.get("err") == Some("1") {
        return Some(Notice::error("ui_login_invalid_credentials"));
    }
    if let Some(key) = known(query.get("err"), &LOGIN_ERRORS) {
        return Some(Notice::error(key));
    }
    let key = if query.has("reset") {
        "ui_login_reset_ok"
    } else if query.has("reset_requested") {
        "ui_login_reset_requested_ok"
    } else if query.has("email_changed") {
        "ui_login_email_changed_ok"
    } else if query.has("invited") {
        "ui_login_invite_accepted_ok"
    } else if query.has("resend_requested") {
        "ui_login_resend_requested_ok"
    } else if query.has("verified") {
        "ui_login_verified_ok"
    } else if query.has("registered") {
        "ui_login_registered_ok"
    } else {
        return None;
    };
    Some(Notice::ok(key))
}

/// Whether the sign-in page offers "send the confirmation link again": where a lost one is the likely problem.
#[must_use]
pub fn login_offers_resend(query: &Query) -> bool {
    query.get("err") == Some("ui_login_email_unverified")
        || query.has("registered")
        || query.has("resend_requested")
}

/// The second step of signing in: a wrong code.
#[must_use]
pub fn login_mfa(query: &Query) -> Option<Notice> {
    (query.get("err") == Some("1")).then(|| Notice::error("ui_login_mfa_invalid"))
}

/// Errors the registration page can be sent back with.
pub const REGISTER_ERRORS: [&str; 7] = [
    "ui_register_error_email_invalid",
    "ui_register_error_rate_limited",
    "ui_admin_error_username_empty",
    "ui_admin_error_username_invalid",
    "ui_admin_error_password_empty",
    "ui_admin_error_exists",
    "ui_admin_error_internal",
];

#[must_use]
pub fn register(query: &Query) -> Option<Notice> {
    known(query.get("err"), &REGISTER_ERRORS).map(Notice::error)
}

/// The reset and invitation pages: a blank password.
#[must_use]
pub fn password_page(query: &Query) -> Option<Notice> {
    known(query.get("err"), &["ui_reset_error_password_empty"]).map(Notice::error)
}

/// Errors the account page can be sent back with.
pub const ACCOUNT_ERRORS: [&str; 13] = [
    "ui_admin_error_password_empty",
    "ui_admin_error_not_found",
    "ui_admin_error_mfa_code_invalid",
    "ui_account_error_current_password",
    "ui_register_error_email_invalid",
    "ui_admin_error_internal",
    "ui_account_error_invalid",
    "ui_account_error_avatar_type",
    "ui_account_error_avatar_size",
    "ui_account_error_password_mismatch",
    "ui_account_error_email_same",
    "ui_account_error_email_not_sent",
    "ui_account_error_rate_limited",
];

#[must_use]
pub fn account(query: &Query) -> Option<Notice> {
    if let Some(key) = known(query.get("err"), &ACCOUNT_ERRORS) {
        return Some(Notice::error(key));
    }
    let key = match query.get("ok")? {
        "saved" => "ui_account_ok_saved",
        "password_changed" => "ui_account_ok_password_changed",
        "mfa_enabled" => "ui_account_ok_mfa_enabled",
        "mfa_disabled" => "ui_account_ok_mfa_disabled",
        "email_change_sent" => "ui_account_ok_email_change_sent",
        "notifications_saved" => "ui_account_ok_notifications_saved",
        _ => return None,
    };
    Some(Notice::ok(key))
}

/// Errors the user administration can be sent back with.
pub const ADMIN_ERRORS: [&str; 16] = [
    "ui_admin_error_not_found",
    "ui_admin_error_username_empty",
    "ui_admin_error_password_empty",
    "ui_admin_error_exists",
    "ui_admin_error_unknown_service",
    "ui_admin_error_last_admin",
    "ui_admin_error_self_demote",
    "ui_admin_error_self_delete",
    "ui_admin_error_self_disable",
    "ui_admin_error_unknown_template",
    "ui_admin_error_roles_require_admin",
    "ui_admin_error_username_invalid",
    "ui_register_error_email_invalid",
    "ui_admin_error_invite_needs_email",
    "ui_admin_error_already_confirmed",
    "ui_admin_error_internal",
];

#[must_use]
pub fn admin(query: &Query) -> Option<Notice> {
    if let Some(key) = known(query.get("err"), &ADMIN_ERRORS) {
        return Some(Notice::error(key));
    }
    Some(match query.get("ok")? {
        "created" => Notice::ok("ui_admin_ok_created"),
        "saved" => Notice::ok("ui_admin_ok_saved"),
        "deleted" => Notice::ok("ui_admin_ok_deleted"),
        "invited" => Notice::ok("ui_admin_ok_invited"),
        // Created, but the email did not go out: not a success to be glossed over.
        "invite_failed" => Notice::error("ui_admin_warn_invite_failed"),
        _ => return None,
    })
}
