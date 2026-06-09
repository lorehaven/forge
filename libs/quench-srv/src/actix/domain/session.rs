use chrono::{Duration, Utc};
use quench_db::prelude::{Crud, Db, Model, Repository};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Session {
    pub id: String,
    pub username: String,
    pub refresh_token_hash: String,
    pub created_at: String,
    pub expires_at: String,
    pub revoked_at: Option<String>,
}

impl Model for Session {
    fn table_name() -> String {
        let schema = envmnt::get_or("DB_SCHEMA", "public");
        format!("{}.sessions", schema)
    }

    fn columns() -> Vec<&'static str> {
        vec![
            "id",
            "username",
            "refresh_token_hash",
            "created_at",
            "expires_at",
            "revoked_at",
        ]
    }

    fn primary_key_name() -> String {
        "id".to_string()
    }
}

pub struct SessionDb {
    repo: Repository<Session>,
}

impl SessionDb {
    pub fn init(db: Db) -> Arc<Self> {
        Arc::new(Self {
            repo: db.repository::<Session>(),
        })
    }

    pub async fn create(&self, username: &str, ttl_secs: i64) -> anyhow::Result<(Session, String)> {
        let refresh_token = new_refresh_token();
        let now = Utc::now();
        let session = Session {
            id: Uuid::new_v4().to_string(),
            username: username.to_string(),
            refresh_token_hash: hash_refresh_token(&refresh_token),
            created_at: now.to_rfc3339(),
            expires_at: (now + Duration::seconds(ttl_secs)).to_rfc3339(),
            revoked_at: None,
        };
        let session = self.repo.create(&session).await?;
        Ok((session, refresh_token))
    }

    pub async fn rotate(
        &self,
        refresh_token: &str,
        ttl_secs: i64,
    ) -> anyhow::Result<Option<(Session, String)>> {
        let Some(mut session) = self.find_active(refresh_token).await? else {
            return Ok(None);
        };
        let next_token = new_refresh_token();
        session.refresh_token_hash = hash_refresh_token(&next_token);
        session.expires_at = (Utc::now() + Duration::seconds(ttl_secs)).to_rfc3339();
        let session = self.repo.update(&session).await?;
        Ok(Some((session, next_token)))
    }

    pub async fn revoke_by_refresh_token(&self, refresh_token: &str) -> anyhow::Result<bool> {
        let Some(mut session) = self.find_active(refresh_token).await? else {
            return Ok(false);
        };
        session.revoked_at = Some(Utc::now().to_rfc3339());
        self.repo.update(&session).await?;
        Ok(true)
    }

    pub async fn revoke(&self, id: &str, username: &str) -> anyhow::Result<bool> {
        let Some(mut session) = self.repo.read(id).await? else {
            return Ok(false);
        };
        if session.username != username || session.revoked_at.is_some() {
            return Ok(false);
        }
        session.revoked_at = Some(Utc::now().to_rfc3339());
        self.repo.update(&session).await?;
        Ok(true)
    }

    pub async fn list_active(&self, username: &str) -> anyhow::Result<Vec<Session>> {
        let now = Utc::now().to_rfc3339();
        let mut sessions: Vec<_> = self
            .repo
            .list()
            .await?
            .into_iter()
            .filter(|session| {
                session.username == username
                    && session.revoked_at.is_none()
                    && session.expires_at > now
            })
            .collect();
        sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(sessions)
    }

    pub async fn is_active(&self, id: &str, username: &str) -> anyhow::Result<bool> {
        let Some(session) = self.repo.read(id).await? else {
            return Ok(false);
        };
        Ok(session.username == username
            && session.revoked_at.is_none()
            && session.expires_at > Utc::now().to_rfc3339())
    }

    async fn find_active(&self, refresh_token: &str) -> anyhow::Result<Option<Session>> {
        let token_hash = hash_refresh_token(refresh_token);
        let now = Utc::now().to_rfc3339();
        Ok(self.repo.list().await?.into_iter().find(|session| {
            session.refresh_token_hash == token_hash
                && session.revoked_at.is_none()
                && session.expires_at > now
        }))
    }
}

fn new_refresh_token() -> String {
    format!("{}.{}", Uuid::new_v4(), Uuid::new_v4())
}

fn hash_refresh_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[actix_web::test]
    async fn refresh_tokens_rotate_and_old_tokens_stop_working() {
        let db = Db::InMemory(quench_db::InMemoryDb::new());
        let sessions = SessionDb::init(db);
        let (session, first_token) = sessions.create("user", 3600).await.unwrap();

        let (_, second_token) = sessions.rotate(&first_token, 3600).await.unwrap().unwrap();

        assert!(sessions.rotate(&first_token, 3600).await.unwrap().is_none());
        assert!(
            sessions
                .rotate(&second_token, 3600)
                .await
                .unwrap()
                .is_some()
        );
        assert!(sessions.is_active(&session.id, "user").await.unwrap());
        assert!(sessions.revoke(&session.id, "user").await.unwrap());
        assert!(!sessions.is_active(&session.id, "user").await.unwrap());
    }
}
