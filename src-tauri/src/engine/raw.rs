//! LocalDeck-internal raw OS facts. Third-party crate structs are converted into
//! these at the provider boundary and never travel further into the engine.

use crate::error::AppResult;
use crate::models::AccessLevel;
use std::net::IpAddr;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct RawProcess {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub name: String,
    pub executable: Option<PathBuf>,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub cpu_usage: f32,
    pub memory_bytes: u64,
    /// Epoch milliseconds.
    pub started_at: Option<i64>,
    pub access: AccessLevel,
}

impl RawProcess {
    /// Classify how much metadata is known (missing data never means "irrelevant").
    pub fn access_from(has_exe: bool, has_args: bool, has_cwd: bool) -> AccessLevel {
        match (has_exe, has_args, has_cwd) {
            (true, true, true) => AccessLevel::Full,
            (false, false, false) => AccessLevel::Unavailable,
            _ => AccessLevel::Partial,
        }
    }

    /// Lower-cased executable basename, falling back to the process name.
    /// Login shells (`-zsh`) are normalised.
    pub fn exe_name(&self) -> String {
        let from_exe = self
            .executable
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string());
        let raw = from_exe.unwrap_or_else(|| self.name.clone());
        raw.trim_start_matches('-').to_lowercase()
    }

    /// Process name as reported by the OS (lower-cased, login dash stripped).
    pub fn name_lower(&self) -> String {
        self.name.trim_start_matches('-').to_lowercase()
    }
}

/// One listening TCP endpoint with every PID the OS associated with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawSocket {
    pub address: IpAddr,
    pub port: u16,
    pub pids: Vec<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SocketScan {
    pub sockets: Vec<RawSocket>,
    /// Sockets/PIDs the OS refused to describe.
    pub skipped: u32,
}

pub trait ProcessProvider: Send {
    fn snapshot(&mut self) -> AppResult<Vec<RawProcess>>;
}

pub trait SocketProvider: Send {
    fn listening_sockets(&mut self) -> AppResult<SocketScan>;
}
