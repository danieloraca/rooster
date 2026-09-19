//! Reviewed, journalled local changes. Provider content is never executed.
mod drafts;
mod files;
mod history;
mod prepare;
mod references;
mod store;
mod types;
pub use drafts::{Draft, DraftComparison, DraftSummary, DraftTarget, edit_reason};
pub use history::{CleanupOutcome, CleanupPreview, GitChange, GitChanges};
pub use store::ChangeStore;
pub use types::{
    Error, FaultPoint, FileKind, FilePreview, HistoryEntry, NewFile, Outcome, OwnerChoice, Preview,
    Request, Result, Status, TextEdit,
};
