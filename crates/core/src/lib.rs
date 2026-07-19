//! Skill Shelf core: content-addressed version control for Claude Agent Skills.
//!
//! A [`Shelf`] stores skills (multi-file directory packages) with git-like
//! versioning — branches, commits, history, diff, rollback — keeping file
//! content in a content-addressed object store and metadata in a SQLite index.
//! It carries no HTTP or UI dependencies so it can be embedded by the server
//! (and, later, other hosts).

mod diff;
mod error;
mod index;
mod models;
mod shelf;
pub mod skillmd;
mod store;

pub use diff::text_diff;
pub use error::{Error, Result};
pub use models::{
    Branch, Commit, Feedback, FileDiff, FileInput, RouteResult, Skill, TreeEntry, User,
    FEEDBACK_APPLIED, FEEDBACK_DISMISSED, FEEDBACK_OPEN, KIND_PROMPT, KIND_SKILL, ROLE_ADMIN,
    ROLE_USER,
};
pub use shelf::{Shelf, DEFAULT_BRANCH};
pub use skillmd::SkillMeta;
