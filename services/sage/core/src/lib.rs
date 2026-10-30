//! Sage's wire model and the rules the browser client applies to it: no I/O, builds for the browser.

pub mod draft;
pub mod markdown;
pub mod model;
pub mod stream;

pub use draft::{
    DraftError, MessageDraft, ProjectDraft, human_bytes, parse_file, preview, upload_result,
};
pub use model::{
    ConversationItem, ConversationView, Created, DefaultModelState, EditBody, FileInfo,
    MessageNode, ModelOption, ModelsView, NewProject, Project, RegenerateBody, Resumed,
    SearchProviderOption, SendBody, Source, SwitchBody, TurnStarted,
};
pub use stream::{Done, Status, StreamEvent};
