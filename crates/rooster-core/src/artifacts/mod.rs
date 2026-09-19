mod inventory;
pub(crate) mod links;
mod scope;
mod snapshot;
mod types;

pub use inventory::inventory;
pub(crate) use inventory::{configured_source_paths, foreign_sources, proposed_kind};
pub use scope::assess as assess_scope;
pub use types::*;
