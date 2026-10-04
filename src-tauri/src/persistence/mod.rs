//! Rust-owned SQLite persistence.
//!
//! The database stores only user intent and history: project identity, user
//! overrides, bounded activity and settings. It never stores process, CPU,
//! RAM or socket data, and never stores environment variables.

mod activity;
mod database;
mod projects;
mod settings;

pub use activity::{NewActivity, ACTIVITY_LIMIT};
pub use database::Database;
pub use projects::ProjectRecord;
