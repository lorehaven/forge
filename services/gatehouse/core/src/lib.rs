//! What Gatehouse's browser pages say: the wire model the service's page API speaks, the notices a redirect carries back to
//! a page, and the drafts (a form's text checked before it is sent).

pub mod draft;
pub mod model;
pub mod notice;

pub use draft::{DraftError, MfaDraft, base_of, file_name, segment};
pub use model::{
    Account, AdminUser, AdminUserRow, AdminUsers, CatalogService, ConfirmEmail, Dashboard, Grant,
    MfaEnrollRequest, MfaEnrollment, ServiceCard, Subscription, UnsubscribeInfo,
};
pub use notice::{Notice, Query, Tone};
