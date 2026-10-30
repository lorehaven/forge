//! What Switchboard's API says and how the launch form is read: the model, the request bodies, the fit arithmetic and the
//! filters. No I/O, and it builds for the browser.

pub mod draft;
pub mod fit;
pub mod model;

pub use draft::{DraftError, FilterDraft, LaunchDraft};
pub use model::{
    Capabilities, Context, DeleteBody, Estimate, Gpu, Instance, LaunchBody, Model, ModelFilters,
};
