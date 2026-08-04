pub mod chunker;
pub mod embedder;
pub mod extractor;
pub mod images;
pub mod pipeline;
pub mod rag;

pub const STATUS_UPLOADED: &str = "uploaded";
pub const STATUS_PROCESSING: &str = "processing";
pub const STATUS_READY: &str = "ready";
pub const STATUS_FAILED: &str = "failed";

/// Whether a stored file is an image attachment; images skip the text extraction/RAG pipeline
/// and are sent to vision models as content parts of the message they're attached to.
pub fn is_image_mime(mime: &str) -> bool {
    mime.starts_with("image/")
}
