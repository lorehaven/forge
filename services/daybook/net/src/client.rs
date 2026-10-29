//! The blocking client: gatehouse for the session (the shared `gatehouse-client`), the daybook service for the tasks.

use std::fmt;
use std::time::Duration;

use daybook_core::{Label, Project, Task};
use quench_serde::Value;

use crate::api::{self, API, label_body, project_body, segment};
use crate::{NetError, check_status as ok};

fn network(e: impl fmt::Display) -> NetError {
    NetError::Network(e.to_string())
}

impl From<gatehouse_client::Error> for NetError {
    fn from(e: gatehouse_client::Error) -> Self {
        use gatehouse_client::Error as E;
        match e {
            E::Network(why) => Self::Network(why),
            E::Unauthorized => Self::Unauthorized,
            E::MfaRequired => Self::MfaRequired,
            E::Status(code) => Self::Status(code),
            E::AccountLocked => Self::AccountLocked,
            E::AccountDisabled => Self::AccountDisabled,
            E::EmailUnverified => Self::EmailUnverified,
        }
    }
}

/// How long a call to a service may take. Set once and before the first call (the first call fixes them).
pub fn set_timeouts(call: Duration, transfer: Duration) {
    gatehouse_client::set_timeouts(call, transfer);
}

/// Signs in. The password is not kept.
///
/// # Errors
/// [`NetError`].
pub fn login(base: &str, username: &str, password: &str) -> Result<Session, NetError> {
    gatehouse_client::trace_var("DAYBOOK_TRACE");
    Ok(Session(gatehouse_client::login(base, username, password)?))
}

/// A signed-in session against one server.
#[derive(Clone, Debug)]
pub struct Session(gatehouse_client::Session);

impl Session {
    /// A session from a refresh token kept from an earlier one.
    #[must_use]
    pub fn resume(base: &str, refresh_token: &str) -> Self {
        gatehouse_client::trace_var("DAYBOOK_TRACE");
        Self(gatehouse_client::Session::resume(base, refresh_token))
    }

    /// The token to store: the new one after every refresh.
    #[must_use]
    pub fn refresh_token(&self) -> &str {
        self.0.refresh_token()
    }

    /// Calls `store` with every new refresh token, as soon as it is given.
    #[must_use]
    pub fn keeping_tokens(self, store: impl Fn(&str) + Send + Sync + 'static) -> Self {
        Self(self.0.keeping_tokens(store))
    }

    /// Asks `latest` for the newest refresh token kept by anyone before a refresh.
    #[must_use]
    pub fn following(self, latest: impl Fn() -> Option<String> + Send + Sync + 'static) -> Self {
        Self(self.0.following(latest))
    }

    /// Gets a new access token (and a new refresh token: the old one is spent).
    ///
    /// # Errors
    /// [`NetError::Unauthorized`] when the refresh token is no good: sign in again.
    pub fn refresh(&mut self) -> Result<(), NetError> {
        Ok(self.0.refresh()?)
    }

    fn read(&mut self, path: &str, query: &[(&str, String)]) -> Result<Value, NetError> {
        let response = self.0.get(&format!("{API}{path}"), query, false)?;
        ok(response.status().as_u16())?;
        response.json().map_err(network)
    }

    fn send(&mut self, method: &str, path: &str, body: Option<&Value>) -> Result<Value, NetError> {
        let path = format!("{API}{path}");
        let url = format!("{}{path}", self.0.base());
        let response = self.0.call(method, &path, false, |client, token| {
            let builder = match method {
                "POST" => client.post(&url),
                "PUT" => client.put(&url),
                _ => client.delete(&url),
            }
            .bearer_auth(token);
            match body {
                Some(body) => builder.json(body).send(),
                None => builder.send(),
            }
        })?;
        ok(response.status().as_u16())?;
        if response.status().as_u16() == 204 {
            return Ok(Value::Null);
        }
        Ok(response.json().unwrap_or(Value::Null))
    }

    /// Every open task, in every list and the inbox.
    ///
    /// # Errors
    /// [`NetError`].
    pub fn open_tasks(&mut self) -> Result<Vec<Task>, NetError> {
        let value = self.read(
            "/tasks",
            &[("all", "true".into()), ("completed", "false".into())],
        )?;
        Ok(api::tasks(&value))
    }

    /// Makes a task from a body built by `daybook_core::Draft::body`.
    ///
    /// # Errors
    /// [`NetError::Invalid`] when the service refuses it.
    pub fn create_task(&mut self, body: &Value) -> Result<Task, NetError> {
        let value = self.send("POST", "/tasks", Some(body))?;
        Task::from_value(&value).ok_or_else(|| NetError::Network("unreadable task".into()))
    }

    /// Replaces a task; the answer is the task as it now is (a repeating task completed comes back open at its next date).
    ///
    /// # Errors
    /// [`NetError::NotFound`] when it is gone.
    pub fn update_task(&mut self, id: &str, body: &Value) -> Result<Task, NetError> {
        let value = self.send("PUT", &format!("/tasks/{}", segment(id)), Some(body))?;
        Task::from_value(&value).ok_or_else(|| NetError::Network("unreadable task".into()))
    }

    /// # Errors
    /// [`NetError`].
    pub fn delete_task(&mut self, id: &str) -> Result<(), NetError> {
        self.send("DELETE", &format!("/tasks/{}", segment(id)), None)
            .map(|_| ())
    }

    /// # Errors
    /// [`NetError`].
    pub fn projects(&mut self) -> Result<Vec<Project>, NetError> {
        let value = self.read("/projects", &[])?;
        Ok(api::projects(&value))
    }

    /// # Errors
    /// [`NetError`].
    pub fn create_project(&mut self, name: &str, sort_order: i64) -> Result<Project, NetError> {
        let value = self.send("POST", "/projects", Some(&project_body(name, sort_order)))?;
        Project::from_value(&value).ok_or_else(|| NetError::Network("unreadable list".into()))
    }

    /// # Errors
    /// [`NetError`].
    pub fn rename_project(
        &mut self,
        id: &str,
        name: &str,
        sort_order: i64,
    ) -> Result<Project, NetError> {
        let value = self.send(
            "PUT",
            &format!("/projects/{}", segment(id)),
            Some(&project_body(name, sort_order)),
        )?;
        Project::from_value(&value).ok_or_else(|| NetError::Network("unreadable list".into()))
    }

    /// Deletes a list **and its tasks**.
    ///
    /// # Errors
    /// [`NetError`].
    pub fn delete_project(&mut self, id: &str) -> Result<(), NetError> {
        self.send("DELETE", &format!("/projects/{}", segment(id)), None)
            .map(|_| ())
    }

    /// # Errors
    /// [`NetError`].
    pub fn labels(&mut self) -> Result<Vec<Label>, NetError> {
        let value = self.read("/labels", &[])?;
        Ok(api::labels(&value))
    }

    /// # Errors
    /// [`NetError::NameTaken`] when a label has that name.
    pub fn create_label(&mut self, name: &str, color: &str) -> Result<Label, NetError> {
        let value = self.send("POST", "/labels", Some(&label_body(name, color)))?;
        Label::from_value(&value).ok_or_else(|| NetError::Network("unreadable label".into()))
    }

    /// # Errors
    /// [`NetError::NameTaken`] when another label has that name.
    pub fn update_label(&mut self, id: &str, name: &str, color: &str) -> Result<Label, NetError> {
        let value = self.send(
            "PUT",
            &format!("/labels/{}", segment(id)),
            Some(&label_body(name, color)),
        )?;
        Label::from_value(&value).ok_or_else(|| NetError::Network("unreadable label".into()))
    }

    /// # Errors
    /// [`NetError`].
    pub fn delete_label(&mut self, id: &str) -> Result<(), NetError> {
        self.send("DELETE", &format!("/labels/{}", segment(id)), None)
            .map(|_| ())
    }
}
