//! Structured application errors.
//!
//! Every fallible Tauri command returns [`AppError`]. It serialises to
//! `{ code, message, context? }` so the frontend can branch on a stable code
//! instead of parsing human text.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Stable, machine-readable error identifiers shared with the frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    ProcessNotFound,
    ProcessPermissionDenied,
    ProjectNotFound,
    PortNotFound,
    PortScanFailed,
    CommandSpawnFailed,
    DatabaseError,
    UnsupportedOperation,
    InvalidInput,
    NoPrimaryCommand,
    ProjectAlreadyRunning,
    ProcessesStillRunning,
    EditorNotFound,
    PathNotFound,
    OpenFailed,
    Internal,
}

/// Wire representation of an error (also embedded in the scanner status).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct ErrorPayload {
    pub code: ErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub context: Option<BTreeMap<String, String>>,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Process {pid} is no longer running")]
    ProcessNotFound { pid: u32 },
    #[error("Not permitted to signal process {pid}")]
    ProcessPermissionDenied { pid: u32 },
    #[error("Project {id} was not found")]
    ProjectNotFound { id: i64 },
    #[error("No listening endpoint with id {id}")]
    PortNotFound { id: String },
    #[error("Socket discovery failed: {0}")]
    PortScanFailed(String),
    #[error("Could not start command: {0}")]
    CommandSpawnFailed(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("{0}")]
    InvalidInput(String),
    #[error("This project has no primary start command. Choose one in the project settings.")]
    NoPrimaryCommand,
    #[error("This project is already running")]
    ProjectAlreadyRunning,
    #[error("{remaining} process(es) are still running after the graceful stop request")]
    ProcessesStillRunning { remaining: u32 },
    #[error("{0}")]
    EditorNotFound(String),
    #[error("Path does not exist: {0}")]
    PathNotFound(String),
    #[error("{0}")]
    OpenFailed(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::ProcessNotFound { .. } => ErrorCode::ProcessNotFound,
            Self::ProcessPermissionDenied { .. } => ErrorCode::ProcessPermissionDenied,
            Self::ProjectNotFound { .. } => ErrorCode::ProjectNotFound,
            Self::PortNotFound { .. } => ErrorCode::PortNotFound,
            Self::PortScanFailed(_) => ErrorCode::PortScanFailed,
            Self::CommandSpawnFailed(_) => ErrorCode::CommandSpawnFailed,
            Self::Database(_) => ErrorCode::DatabaseError,
            Self::Unsupported(_) => ErrorCode::UnsupportedOperation,
            Self::InvalidInput(_) => ErrorCode::InvalidInput,
            Self::NoPrimaryCommand => ErrorCode::NoPrimaryCommand,
            Self::ProjectAlreadyRunning => ErrorCode::ProjectAlreadyRunning,
            Self::ProcessesStillRunning { .. } => ErrorCode::ProcessesStillRunning,
            Self::EditorNotFound(_) => ErrorCode::EditorNotFound,
            Self::PathNotFound(_) => ErrorCode::PathNotFound,
            Self::OpenFailed(_) => ErrorCode::OpenFailed,
            Self::Internal(_) => ErrorCode::Internal,
        }
    }

    /// Safe, small key/value context (never raw command lines or env).
    fn context(&self) -> Option<BTreeMap<String, String>> {
        let mut ctx = BTreeMap::new();
        match self {
            Self::ProcessNotFound { pid } | Self::ProcessPermissionDenied { pid } => {
                ctx.insert("pid".to_string(), pid.to_string());
            }
            Self::ProjectNotFound { id } => {
                ctx.insert("projectId".to_string(), id.to_string());
            }
            Self::PortNotFound { id } => {
                ctx.insert("portId".to_string(), id.clone());
            }
            Self::ProcessesStillRunning { remaining } => {
                ctx.insert("remaining".to_string(), remaining.to_string());
            }
            Self::PathNotFound(path) => {
                ctx.insert("path".to_string(), path.clone());
            }
            _ => return None,
        }
        Some(ctx)
    }

    pub fn payload(&self) -> ErrorPayload {
        ErrorPayload {
            code: self.code(),
            message: self.to_string(),
            context: self.context(),
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.payload().serialize(serializer)
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(err: rusqlite::Error) -> Self {
        Self::Database(err.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialises_code_message_and_context() {
        let json = serde_json::to_value(AppError::ProcessNotFound { pid: 4242 }).unwrap();
        assert_eq!(json["code"], "PROCESS_NOT_FOUND");
        assert_eq!(json["message"], "Process 4242 is no longer running");
        assert_eq!(json["context"]["pid"], "4242");
    }

    #[test]
    fn omits_context_when_there_is_none() {
        let json = serde_json::to_value(AppError::NoPrimaryCommand).unwrap();
        assert_eq!(json["code"], "NO_PRIMARY_COMMAND");
        assert!(json.get("context").is_none());
    }

    #[test]
    fn every_database_failure_maps_to_database_error() {
        let err: AppError = rusqlite::Error::QueryReturnedNoRows.into();
        assert_eq!(err.code(), ErrorCode::DatabaseError);
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], "DATABASE_ERROR");
    }

    #[test]
    fn payload_round_trips_through_json() {
        let payload = AppError::ProcessPermissionDenied { pid: 7 }.payload();
        let back: ErrorPayload =
            serde_json::from_str(&serde_json::to_string(&payload).unwrap()).unwrap();
        assert_eq!(back, payload);
    }
}
