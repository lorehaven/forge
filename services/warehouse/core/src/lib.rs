//! What Warehouse's management API (`/ui/api`) says and how a form is read: the model, the request bodies, the drafts, the
//! registry trees and the file tree. No I/O, and it builds for the browser: the service and the client share it.

pub mod draft;
pub mod fmt;
pub mod model;
pub mod tree;

pub use draft::{DraftError, EditStorageDraft, NewStorageDraft};
pub use model::{
    ArtifactEntry, ArtifactRef, Browse, BrowseFile, CrateDep, CrateRef, CrateVersion, DockerTag,
    DynamicStorage, Features, FileRef, ImageRef, MetaPair, NewStorage, Overview, RivetEntry,
    RivetRef, StaticStorage, StorageChanges, Storages,
};
