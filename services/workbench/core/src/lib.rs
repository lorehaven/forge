//! What Workbench's API says and how a form is read: the model, the request bodies, the drafts, the board.

pub mod board;
pub mod draft;
pub mod model;

pub use draft::{CommentDraft, DraftError, IssueDraft, LinkDraft, ProjectDraft};
pub use model::{
    Board, Comment, CreateIssue, CreateProject, Issue, IssueView, LinkedIssue, Me, NewComment,
    NewLink, Project, RelatedIssues, Transition, UpdateIssue, User,
};
