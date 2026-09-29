//! A cap on how many emails the whole estate sends per day.
//!
//! The relay behind the mail server has a small daily allowance. Per-client
//! limits keep one abuser bounded; this keeps a crowd of them from using up the
//! allowance that legitimate verification and reset mail also depends on. It
//! wraps the real sender only - the logging sender costs nothing.

use super::{Mail, Recipient, SendError, Sender};
use crate::ratelimit::{Limit, RateLimiter, Verdict};
use async_trait::async_trait;
use std::sync::Arc;

pub struct BudgetedSender {
    inner: Arc<dyn Sender>,
    limiter: RateLimiter,
    per_day: usize,
}

impl BudgetedSender {
    pub fn new(inner: Arc<dyn Sender>, limiter: RateLimiter, per_day: usize) -> Self {
        Self {
            inner,
            limiter,
            per_day,
        }
    }

    /// Spends one email from today's budget (UTC day), or says it is gone.
    async fn spend(&self) -> Result<(), SendError> {
        match self
            .limiter
            .check("mail-budget", "global", Limit::per_day(self.per_day))
            .await
        {
            Verdict::Allowed => Ok(()),
            Verdict::Limited { retry_after_secs } => {
                tracing::warn!(
                    "mail: daily budget of {} emails used up, not sending (resets in {retry_after_secs}s)",
                    self.per_day
                );
                Err(SendError::transient("the daily email budget is used up"))
            }
        }
    }
}

#[async_trait]
impl Sender for BudgetedSender {
    async fn send(&self, to: &Recipient<'_>, mail: &Mail<'_>) -> Result<(), SendError> {
        self.spend().await?;
        self.inner.send(to, mail).await
    }
}
