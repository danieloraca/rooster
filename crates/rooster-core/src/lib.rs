//! Filesystem-backed workspace configuration and read-only Git discovery.

pub mod artifacts;
pub mod changes;
mod config;
mod git;
mod paths;
pub mod providers;
mod scan;

pub use config::{Config, ConfigStore, Error, Registration, Root, Workspace};
pub use paths::NativePath;
pub use scan::{
    CancellationToken, Checkout, CheckoutKind, HeadState, ScanIssue, ScanOptions, ScanProgress,
    ScanReport, ScanStatus, SkipReason, SkippedPath, WorktreeCandidate, scan,
};
