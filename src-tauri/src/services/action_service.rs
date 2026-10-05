//! "Open …" actions. All OS behaviour is delegated to `platform`.

use super::runtime::{lock, Runtime};
use crate::error::{AppError, AppResult};
use crate::models::ProjectId;
use crate::platform;
use std::path::PathBuf;

impl Runtime {
    pub fn open_project_url(&self, id: ProjectId) -> AppResult<()> {
        let project = self.find_project(id)?;
        let url = project.primary_url.ok_or_else(|| {
            AppError::InvalidInput(
                "This project has no URL yet. Start it or set a primary URL.".to_string(),
            )
        })?;
        platform::open_url(&url.url)
    }

    /// Open a discovered HTTP endpoint (by its id in the latest snapshot).
    pub fn open_port_url(&self, port_id: &str) -> AppResult<()> {
        let snapshot = self.snapshot();
        let port = snapshot
            .ports
            .iter()
            .find(|p| p.id == port_id)
            .ok_or_else(|| AppError::PortNotFound {
                id: port_id.to_string(),
            })?;
        let url = port.url.as_ref().ok_or_else(|| {
            AppError::InvalidInput(format!(
                "Port {} is not a confirmed HTTP endpoint",
                port.port
            ))
        })?;
        platform::open_url(url)
    }

    pub fn open_project_folder(&self, id: ProjectId) -> AppResult<()> {
        let project = self.find_project(id)?;
        platform::open_folder(&PathBuf::from(project.root_path))
    }

    pub fn open_project_terminal(&self, id: ProjectId) -> AppResult<()> {
        let project = self.find_project(id)?;
        // Project choice → global setting → platform default.
        let terminal = project
            .overrides
            .preferred_terminal
            .clone()
            .or_else(|| lock(&self.settings).preferred_terminal.clone());
        platform::open_terminal(&PathBuf::from(project.root_path), terminal.as_deref())
    }

    /// Project editor override → global preference → first detected editor.
    pub fn open_project_editor(&self, id: ProjectId) -> AppResult<()> {
        let project = self.find_project(id)?;
        let editor = project
            .overrides
            .preferred_editor
            .clone()
            .or_else(|| lock(&self.settings).preferred_editor.clone())
            .or_else(platform::default_editor_id)
            .ok_or_else(|| {
                AppError::EditorNotFound(
                    "No supported editor was found. Choose one in Settings.".to_string(),
                )
            })?;
        platform::open_in_editor(&editor, &PathBuf::from(project.root_path))
    }

    /// Open the working directory of a process shown in the UI.
    pub fn open_process_cwd(&self, pid: u32) -> AppResult<()> {
        let snapshot = self.snapshot();
        let cwd = snapshot
            .projects
            .iter()
            .flat_map(|p| p.processes.iter())
            .find(|p| p.pid == pid)
            .and_then(|p| p.cwd.clone())
            .ok_or(AppError::ProcessNotFound { pid })?;
        platform::open_folder(&PathBuf::from(cwd))
    }
}
