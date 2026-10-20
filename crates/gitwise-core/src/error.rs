use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// A git command that exited with a code we did not expect.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitFailure {
    pub command: String,
    pub code: i32,
    pub stderr: String,
    /// What went wrong in plain terms (see [`crate::git::classify_failure`]); the UI turns this
    /// into an explanation and a suggested next step.
    pub kind: Option<&'static str>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Git is not installed or could not be found")]
    GitNotFound,
    #[error("git failed ({}): {}", .0.code, .0.stderr.trim())]
    Git(GitFailure),
    #[error("not a git repository: {0}")]
    NotARepo(String),
    #[error("profile not found: {0}")]
    ProfileNotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error("credential store error: {0}")]
    Secret(String),
    #[error("network error: {0}")]
    Http(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Error::GitNotFound => "git_not_found",
            Error::Git(_) => "git",
            Error::NotARepo(_) => "not_a_repo",
            Error::ProfileNotFound(_) => "profile_not_found",
            Error::Invalid(_) => "invalid",
            Error::Secret(_) => "secret",
            Error::Http(_) => "http",
            Error::Io(_) => "io",
            Error::Json(_) => "json",
        }
    }

    pub fn invalid(msg: impl Into<String>) -> Self {
        Error::Invalid(msg.into())
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Http(e.to_string())
    }
}

impl From<keyring::Error> for Error {
    fn from(e: keyring::Error) -> Self {
        Error::Secret(e.to_string())
    }
}

/// Errors cross into the UI as `{ code, message, git }`.
impl Serialize for Error {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut st = s.serialize_struct("Error", 3)?;
        st.serialize_field("code", self.code())?;
        st.serialize_field("message", &self.to_string())?;
        let git = match self {
            Error::Git(g) => Some(g),
            _ => None,
        };
        st.serialize_field("git", &git)?;
        st.end()
    }
}
