//! Gitwise engine.
//!
//! Everything that touches git goes through [`git::GitRunner`], which runs the real `git`
//! binary (so LFS, hooks, sparse checkout and every other extension behave exactly as they do
//! in a terminal). On top of that sit profiles (who you are), rules (which profile a repository
//! belongs to) and the guard (stops a commit or push that would go out under the wrong account).

pub mod askpass;
pub mod error;
pub mod git;
pub mod guard;
pub mod log;
pub mod ops;
pub mod profile;
pub mod providers;
pub mod remote;
pub mod rules;
pub mod secrets;
pub mod status;
pub mod store;
pub mod tools;
pub mod util;

pub use error::{Error, Result};
