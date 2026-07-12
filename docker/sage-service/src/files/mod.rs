pub mod chunker;
pub mod embedder;
pub mod extractor;
pub mod pipeline;
pub mod rag;

pub const STATUS_UPLOADED: &str = "uploaded";
pub const STATUS_PROCESSING: &str = "processing";
pub const STATUS_READY: &str = "ready";
pub const STATUS_FAILED: &str = "failed";
