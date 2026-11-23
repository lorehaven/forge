//! What Gantry's API says and how a form is read: the model, the request bodies, the filters, the YAML draft.

pub mod draft;
pub mod filter;
pub mod format;
pub mod model;

pub use draft::{DraftError, EditDraft};
pub use model::{
    Busy, ClusterEntry, ClusterEvent, ClusterKind, ClusterObject, ClusterOverview, ClusterRef,
    DeploymentPlan, DeploymentView, EditBody, ExecRead, FacadeEntry, Group, Info, Operation,
    PackageBody, PackageVersion, Plan, PlanRequest, ResourceBody, ResourceRef, ResourceYaml, Row,
    StoredPlan, Summary, TargetView, Unit, kinds_by_group, plain_terminal,
};
