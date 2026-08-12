//! A cache you can point at Redis or keep in the process.
//!
//! [`DataCache`] lives inside one process, which is the right thing for
//! caching an outbound HTTP response and the wrong thing for anything that has
//! to be consistent across replicas: an entry invalidated on one pod stays
//! valid on every other until its TTL runs out. [`CacheStore`] is the shared
//! shape - the same calls against either backend - so that choice becomes
//! configuration rather than a rewrite.

use crate::CacheError;
use serde_json::Value;
use std::sync::Arc;

#[cfg(feature = "redis")]
use redis::AsyncCommands;

/// Where cached values live.
#[derive(Clone)]
pub enum CacheStore {
    /// Per-process, backed by [`DataCache`]. Fast, and invisible to other
    /// replicas.
    InMemory(Arc<crate::data_cache::DataCache>),

    /// Shared, backed by Redis. Survives a restart and is visible estate-wide.
    #[cfg(feature = "redis")]
    Redis(RedisStore),
}

impl CacheStore {
    pub fn in_memory() -> Self {
        Self::InMemory(Arc::new(crate::data_cache::DataCache::new()))
    }

    /// Redis when `CACHE_URL` (or `REDIS_URL`) is set, in-process otherwise.
    ///
    /// A comma-separated URL is read as cluster seed nodes; `CACHE_CLUSTER=true`
    /// (or `REDIS_CLUSTER=true`) forces cluster mode when you only have one seed
    /// to hand.
    ///
    /// Without the `redis` feature a configured URL is an error rather than a
    /// silent downgrade: a deployment that asked for a shared cache and got a
    /// per-process one would be wrong in a way nobody notices until two
    /// replicas disagree.
    pub async fn from_env(key_prefix: impl Into<String>) -> Result<Self, CacheError> {
        let url = ["CACHE_URL", "REDIS_URL"]
            .iter()
            .find_map(|key| std::env::var(key).ok())
            .filter(|value| !value.trim().is_empty());

        match url {
            None => Ok(Self::in_memory()),
            #[cfg(feature = "redis")]
            Some(url) => {
                let forced = ["CACHE_CLUSTER", "REDIS_CLUSTER"]
                    .iter()
                    .find_map(|key| std::env::var(key).ok())
                    .is_some_and(|value| value.trim().eq_ignore_ascii_case("true"));
                Ok(Self::Redis(
                    RedisStore::connect_with(&url, key_prefix, forced).await?,
                ))
            }
            #[cfg(not(feature = "redis"))]
            Some(url) => {
                let _ = key_prefix;
                Err(CacheError::Backend(format!(
                    "a cache URL is configured ({url}) but quench-cache was built \
                     without the `redis` feature"
                )))
            }
        }
    }

    pub async fn get(&self, key: &str) -> Result<Option<Value>, CacheError> {
        match self {
            Self::InMemory(cache) => Ok(cache.get(key)),
            #[cfg(feature = "redis")]
            Self::Redis(store) => store.get(key).await,
        }
    }

    /// Stores `value`, expiring after `ttl_secs` when given.
    pub async fn set(
        &self,
        key: &str,
        value: Value,
        ttl_secs: Option<u64>,
    ) -> Result<(), CacheError> {
        match self {
            Self::InMemory(cache) => {
                match ttl_secs {
                    Some(ttl) => cache.set_with_ttl(key.to_string(), value, ttl),
                    None => cache.set(key.to_string(), value),
                }
                Ok(())
            }
            #[cfg(feature = "redis")]
            Self::Redis(store) => store.set(key, value, ttl_secs).await,
        }
    }

    /// Reads and removes in one step.
    ///
    /// This is what makes a single-use token single-use: two callers racing on
    /// the same key, exactly one gets the value.
    pub async fn take(&self, key: &str) -> Result<Option<Value>, CacheError> {
        match self {
            Self::InMemory(cache) => {
                let value = cache.get(key);
                cache.delete(key);
                Ok(value)
            }
            #[cfg(feature = "redis")]
            Self::Redis(store) => store.take(key).await,
        }
    }

    pub async fn remove(&self, key: &str) -> Result<(), CacheError> {
        match self {
            Self::InMemory(cache) => {
                cache.delete(key);
                Ok(())
            }
            #[cfg(feature = "redis")]
            Self::Redis(store) => store.remove(key).await,
        }
    }

    /// Adds `member` to the set at `key`, refreshing the set's lifetime.
    ///
    /// A set rather than a value because the members arrive from concurrent
    /// writers - several sessions for one user - and read-modify-write over a
    /// JSON array would drop one of two simultaneous additions.
    pub async fn add_to_set(
        &self,
        key: &str,
        member: &str,
        ttl_secs: Option<u64>,
    ) -> Result<(), CacheError> {
        match self {
            Self::InMemory(cache) => {
                cache.add_to_set(key, member, ttl_secs);
                Ok(())
            }
            #[cfg(feature = "redis")]
            Self::Redis(store) => store.add_to_set(key, member, ttl_secs).await,
        }
    }

    /// Members of the set at `key`, in no particular order. A missing set reads
    /// as empty rather than as an error - to a caller they mean the same thing.
    pub async fn set_members(&self, key: &str) -> Result<Vec<String>, CacheError> {
        match self {
            Self::InMemory(cache) => Ok(cache.set_members(key)),
            #[cfg(feature = "redis")]
            Self::Redis(store) => store.set_members(key).await,
        }
    }

    pub async fn remove_from_set(&self, key: &str, member: &str) -> Result<(), CacheError> {
        match self {
            Self::InMemory(cache) => {
                cache.remove_from_set(key, member);
                Ok(())
            }
            #[cfg(feature = "redis")]
            Self::Redis(store) => store.remove_from_set(key, member).await,
        }
    }

    /// Drops everything this store owns. On Redis that is the configured key
    /// prefix only - never the whole database, which may not be yours.
    pub async fn clear(&self) -> Result<(), CacheError> {
        match self {
            Self::InMemory(cache) => {
                cache.clear();
                Ok(())
            }
            #[cfg(feature = "redis")]
            Self::Redis(store) => store.clear().await,
        }
    }

    /// Whether entries are visible to other processes. Worth checking before
    /// relying on this for anything that must be consistent estate-wide.
    pub const fn is_shared(&self) -> bool {
        match self {
            Self::InMemory(_) => false,
            #[cfg(feature = "redis")]
            Self::Redis(_) => true,
        }
    }

    /// What this store is talking to, for the startup log - an operator should
    /// be able to tell a clustered pod from a standalone one without guessing.
    pub const fn topology(&self) -> &'static str {
        match self {
            Self::InMemory(_) => "in-process memory",
            #[cfg(feature = "redis")]
            Self::Redis(store) if store.is_clustered() => "a Redis cluster",
            #[cfg(feature = "redis")]
            Self::Redis(_) => "a single Redis server",
        }
    }
}

/// One server or a cluster.
///
/// The single-server case holds a [`redis::aio::ConnectionManager`] rather
/// than a bare `MultiplexedConnection`: the latter never recovers once its
/// socket dies, so a Redis pod restart would wedge every holder of one until
/// their process restarted too. `ConnectionManager` wraps the same
/// multiplexed connection but reconnects in the background on I/O errors.
///
/// Implementing `ConnectionLike` over the pair is what keeps the commands below
/// written once: `AsyncCommands` is blanket-implemented for anything that is
/// `ConnectionLike`, so `get`/`set`/`del` do not care which we hold. Only
/// [`RedisStore::clear`] has to know, because `SCAN` is per-node.
#[cfg(feature = "redis")]
#[derive(Clone)]
enum Connection {
    Single(redis::aio::ConnectionManager),
    Cluster(redis::cluster_async::ClusterConnection),
}

#[cfg(feature = "redis")]
impl redis::aio::ConnectionLike for Connection {
    fn req_packed_command<'a>(
        &'a mut self,
        cmd: &'a redis::Cmd,
    ) -> redis::RedisFuture<'a, redis::Value> {
        match self {
            Self::Single(connection) => connection.req_packed_command(cmd),
            Self::Cluster(connection) => connection.req_packed_command(cmd),
        }
    }

    fn req_packed_commands<'a>(
        &'a mut self,
        cmd: &'a redis::Pipeline,
        offset: usize,
        count: usize,
    ) -> redis::RedisFuture<'a, Vec<redis::Value>> {
        match self {
            Self::Single(connection) => connection.req_packed_commands(cmd, offset, count),
            Self::Cluster(connection) => connection.req_packed_commands(cmd, offset, count),
        }
    }

    fn get_db(&self) -> i64 {
        match self {
            Self::Single(connection) => connection.get_db(),
            Self::Cluster(connection) => connection.get_db(),
        }
    }
}

#[cfg(feature = "redis")]
#[derive(Clone)]
pub struct RedisStore {
    connection: Connection,
    /// Namespaces every key, so several services can share one Redis without
    /// colliding, and `clear` has something to scope itself to.
    prefix: String,
}

#[cfg(feature = "redis")]
impl RedisStore {
    /// Connects, reading a comma-separated `url` as cluster seed nodes.
    pub async fn connect(url: &str, prefix: impl Into<String>) -> Result<Self, CacheError> {
        Self::connect_with(url, prefix, false).await
    }

    /// Same, but `cluster` forces cluster mode even for a single seed - a
    /// cluster you reach through one address is indistinguishable from a
    /// standalone server by URL alone.
    pub async fn connect_with(
        url: &str,
        prefix: impl Into<String>,
        cluster: bool,
    ) -> Result<Self, CacheError> {
        let seeds: Vec<&str> = url
            .split(',')
            .map(str::trim)
            .filter(|seed| !seed.is_empty())
            .collect();

        if seeds.is_empty() {
            return Err(CacheError::Backend("no Redis URL configured".into()));
        }

        let connection = if cluster || seeds.len() > 1 {
            let client = redis::cluster::ClusterClient::new(seeds)
                .map_err(|err| CacheError::Backend(format!("invalid Redis cluster URL: {err}")))?;
            Connection::Cluster(client.get_async_connection().await.map_err(|err| {
                CacheError::Backend(format!("Redis cluster connection failed: {err}"))
            })?)
        } else {
            let client = redis::Client::open(seeds[0])
                .map_err(|err| CacheError::Backend(format!("invalid Redis URL: {err}")))?;
            Connection::Single(
                client.get_connection_manager().await.map_err(|err| {
                    CacheError::Backend(format!("Redis connection failed: {err}"))
                })?,
            )
        };

        Ok(Self {
            connection,
            prefix: prefix.into(),
        })
    }

    /// Whether this store is talking to a cluster.
    pub const fn is_clustered(&self) -> bool {
        matches!(self.connection, Connection::Cluster(_))
    }

    fn qualified(&self, key: &str) -> String {
        if self.prefix.is_empty() {
            key.to_string()
        } else {
            format!("{}:{key}", self.prefix)
        }
    }

    pub async fn get(&self, key: &str) -> Result<Option<Value>, CacheError> {
        let mut connection = self.connection.clone();
        let raw: Option<String> = connection
            .get(self.qualified(key))
            .await
            .map_err(|err| CacheError::Backend(err.to_string()))?;

        raw.map(|value| {
            serde_json::from_str(&value)
                .map_err(|err| CacheError::SerializationError(err.to_string()))
        })
        .transpose()
    }

    pub async fn set(
        &self,
        key: &str,
        value: Value,
        ttl_secs: Option<u64>,
    ) -> Result<(), CacheError> {
        let mut connection = self.connection.clone();
        let encoded = serde_json::to_string(&value)
            .map_err(|err| CacheError::SerializationError(err.to_string()))?;

        match ttl_secs {
            Some(ttl) => connection
                .set_ex::<_, _, ()>(self.qualified(key), encoded, ttl)
                .await
                .map_err(|err| CacheError::Backend(err.to_string())),
            None => connection
                .set::<_, _, ()>(self.qualified(key), encoded)
                .await
                .map_err(|err| CacheError::Backend(err.to_string())),
        }
    }

    /// `GETDEL`: atomic on the server, so concurrent callers cannot both win.
    pub async fn take(&self, key: &str) -> Result<Option<Value>, CacheError> {
        let mut connection = self.connection.clone();
        let raw: Option<String> = connection
            .get_del(self.qualified(key))
            .await
            .map_err(|err| CacheError::Backend(err.to_string()))?;

        raw.map(|value| {
            serde_json::from_str(&value)
                .map_err(|err| CacheError::SerializationError(err.to_string()))
        })
        .transpose()
    }

    pub async fn remove(&self, key: &str) -> Result<(), CacheError> {
        let mut connection = self.connection.clone();
        connection
            .del::<_, ()>(self.qualified(key))
            .await
            .map_err(|err| CacheError::Backend(err.to_string()))
    }

    /// `SADD`, then `EXPIRE` when a lifetime is given.
    ///
    /// Two commands rather than one, and not in a transaction: a set that ends
    /// up without its expiry is a key that outlives its usefulness, which is
    /// worse than nothing but not wrong. Both are single-key, so a cluster
    /// routes them to the same node.
    pub async fn add_to_set(
        &self,
        key: &str,
        member: &str,
        ttl_secs: Option<u64>,
    ) -> Result<(), CacheError> {
        let mut connection = self.connection.clone();
        let qualified = self.qualified(key);

        connection
            .sadd::<_, _, ()>(&qualified, member)
            .await
            .map_err(|err| CacheError::Backend(err.to_string()))?;

        if let Some(ttl) = ttl_secs {
            connection
                .expire::<_, ()>(&qualified, ttl as i64)
                .await
                .map_err(|err| CacheError::Backend(err.to_string()))?;
        }
        Ok(())
    }

    pub async fn set_members(&self, key: &str) -> Result<Vec<String>, CacheError> {
        let mut connection = self.connection.clone();
        connection
            .smembers(self.qualified(key))
            .await
            .map_err(|err| CacheError::Backend(err.to_string()))
    }

    /// `SREM`. Redis drops a set once its last member leaves, so there is no
    /// empty key left behind to clean up.
    pub async fn remove_from_set(&self, key: &str, member: &str) -> Result<(), CacheError> {
        let mut connection = self.connection.clone();
        connection
            .srem::<_, _, ()>(self.qualified(key), member)
            .await
            .map_err(|err| CacheError::Backend(err.to_string()))
    }

    /// Deletes this store's keys with `SCAN`, which - unlike `KEYS` - does not
    /// block the server while it walks the keyspace.
    ///
    /// On a cluster this walks every primary in turn: `SCAN` only ever reports
    /// the keyspace of the node that answers it, and a cursor from one node
    /// means nothing to another.
    pub async fn clear(&self) -> Result<(), CacheError> {
        let pattern = self.qualified("*");

        match &self.connection {
            Connection::Single(_) => {
                let mut connection = self.connection.clone();
                Self::sweep(&mut connection, &pattern, None).await
            }
            Connection::Cluster(cluster) => {
                let mut connection = self.connection.clone();
                for node in Self::primaries(&mut cluster.clone()).await? {
                    Self::sweep(&mut connection, &pattern, Some(node)).await?;
                }
                Ok(())
            }
        }
    }

    /// Walks `pattern` on one node and deletes what it finds.
    ///
    /// `node` picks the primary to scan; without it the command goes wherever
    /// the connection would normally send it, which is the whole keyspace on a
    /// standalone server.
    async fn sweep(
        connection: &mut Connection,
        pattern: &str,
        node: Option<(String, u16)>,
    ) -> Result<(), CacheError> {
        let mut cursor: u64 = 0;

        loop {
            let mut scan = redis::cmd("SCAN");
            scan.arg(cursor)
                .arg("MATCH")
                .arg(pattern)
                .arg("COUNT")
                .arg(500);

            let (next, keys): (u64, Vec<String>) = match (&mut *connection, node.clone()) {
                (Connection::Cluster(cluster), Some((host, port))) => {
                    let value = cluster
                        .route_command(
                            scan,
                            redis::cluster_routing::RoutingInfo::SingleNode(
                                redis::cluster_routing::SingleNodeRoutingInfo::ByAddress {
                                    host,
                                    port,
                                },
                            ),
                        )
                        .await
                        .map_err(|err| CacheError::Backend(err.to_string()))?;
                    redis::from_redis_value(value)
                        .map_err(|err| CacheError::Backend(err.to_string()))?
                }
                (connection, _) => scan
                    .query_async(connection)
                    .await
                    .map_err(|err| CacheError::Backend(err.to_string()))?,
            };

            // One key per `DEL`: on a cluster a multi-key delete is rejected
            // outright unless every key hashes to the same slot.
            for key in keys {
                connection
                    .del::<_, ()>(key)
                    .await
                    .map_err(|err| CacheError::Backend(err.to_string()))?;
            }

            cursor = next;
            if cursor == 0 {
                return Ok(());
            }
        }
    }

    /// The cluster's primaries, from `CLUSTER NODES`.
    ///
    /// Asking the cluster rather than trusting the configured seeds means a
    /// node that joined after startup is still swept.
    async fn primaries(
        cluster: &mut redis::cluster_async::ClusterConnection,
    ) -> Result<Vec<(String, u16)>, CacheError> {
        let mut cmd = redis::cmd("CLUSTER");
        cmd.arg("NODES");
        let value = cluster
            .route_command(
                cmd,
                redis::cluster_routing::RoutingInfo::SingleNode(
                    redis::cluster_routing::SingleNodeRoutingInfo::RandomPrimary,
                ),
            )
            .await
            .map_err(|err| CacheError::Backend(format!("CLUSTER NODES failed: {err}")))?;

        let raw: String =
            redis::from_redis_value(value).map_err(|err| CacheError::Backend(err.to_string()))?;

        Ok(raw.lines().filter_map(parse_primary).collect())
    }
}

/// Reads one `CLUSTER NODES` line, keeping healthy primaries.
///
/// The address field is `host:port@cport[,hostname[,aux=…]]`; the flags field
/// is a comma-separated list that contains `master` or `slave`, plus `fail` or
/// `fail?` for a node the cluster has given up on.
#[cfg(feature = "redis")]
#[doc(hidden)]
pub fn parse_primary(line: &str) -> Option<(String, u16)> {
    let mut fields = line.split_whitespace();
    let _id = fields.next()?;
    let address = fields.next()?;
    let flags = fields.next()?;

    let healthy_primary = flags
        .split(',')
        .any(|flag| flag == "master" || flag == "primary")
        && !flags.split(',').any(|flag| flag.starts_with("fail"));
    if !healthy_primary {
        return None;
    }

    let address = address.split('@').next()?;
    let (host, port) = address.rsplit_once(':')?;
    Some((host.to_string(), port.parse().ok()?))
}
