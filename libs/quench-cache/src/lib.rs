use thiserror::Error;

#[derive(Error, Debug)]
pub enum CacheError {
    #[error("Cache miss for key: {0}")]
    Miss(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
}

pub type Result<T> = std::result::Result<T, CacheError>;

#[cfg(feature = "data-cache")]
pub mod data_cache;
#[cfg(feature = "request-cache")]
pub mod request_cache;

#[cfg(feature = "data-cache")]
pub use data_cache::DataCache;
#[cfg(feature = "request-cache")]
pub use request_cache::RequestCache;

pub mod prelude {
    #[cfg(feature = "data-cache")]
    pub use crate::data_cache::DataCache;
    #[cfg(feature = "request-cache")]
    pub use crate::request_cache::RequestCache;
}
