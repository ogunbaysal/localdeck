//! Deterministic project-root detection from a process working directory.
//!
//! Precedence (docs/engine/01-discovery-and-classification.md):
//! 1. nearest enclosing Git root (`.git` directory **or** worktree/submodule
//!    `.git` file);
//! 2. otherwise nearest workspace root;
//! 3. otherwise nearest standalone manifest root;
//! 4. otherwise no project is fabricated.
//!
//! `/` and the user's home directory (and anything above it) are never roots,
//! so a dotfiles repo at `~` cannot swallow every shell in the home folder.

use crate::models::RootKind;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_ANCESTORS: usize = 48;

const WORKSPACE_FILES: &[&str] = &[
    "pnpm-workspace.yaml",
    "turbo.json",
    "nx.json",
    "lerna.json",
    "go.work",
];

const MANIFEST_FILES: &[&str] = &[
    "package.json",
    "Cargo.toml",
    "composer.json",
    "pyproject.toml",
    "go.mod",
    "docker-compose.yml",
    "docker-compose.yaml",
    "compose.yml",
    "compose.yaml",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRoot {
    pub path: PathBuf,
    pub kind: RootKind,
}

/// System- and package-manager-owned locations. Their git checkouts (for
/// example Homebrew's own repository at `/opt/homebrew`) and everything under
/// them are infrastructure, never user projects: services whose cwd lives
/// there (brew services, daemons) stay unassigned.
const SYSTEM_LOCATIONS: &[&str] = &[
    "/opt/homebrew",
    "/usr/local/Homebrew",
    "/home/linuxbrew/.linuxbrew",
    "/System",
    "/Library",
    "/usr",
    "/etc",
    "/bin",
    "/sbin",
    "/var/lib",
    "/nix",
    "/snap",
];

#[derive(Debug, Clone)]
pub struct DetectorConfig {
    home: Option<PathBuf>,
    system_locations: Vec<PathBuf>,
}

impl Default for DetectorConfig {
    fn default() -> Self {
        Self::new(None)
    }
}

impl DetectorConfig {
    pub fn new(home: Option<PathBuf>) -> Self {
        Self {
            home: home.map(|h| canonicalize_lenient(&h)),
            system_locations: SYSTEM_LOCATIONS.iter().map(PathBuf::from).collect(),
        }
    }

    fn is_system_location(&self, path: &Path) -> bool {
        self.system_locations
            .iter()
            .any(|root| path.starts_with(root))
    }
}

/// Uncached detection for a single working directory.
pub fn detect_root(cwd: &Path, config: &DetectorConfig) -> Option<ProjectRoot> {
    let canonical = canonicalize_lenient(cwd);
    if config.is_system_location(&canonical) {
        return None;
    }
    let mut workspace: Option<PathBuf> = None;
    let mut manifest: Option<PathBuf> = None;

    for dir in canonical.ancestors().take(MAX_ANCESTORS) {
        if dir.parent().is_none() || config.home.as_deref() == Some(dir) {
            break;
        }
        if is_git_root(dir) {
            return Some(ProjectRoot {
                path: dir.to_path_buf(),
                kind: RootKind::Git,
            });
        }
        if workspace.is_none() && is_workspace_root(dir) {
            workspace = Some(dir.to_path_buf());
        }
        if manifest.is_none() && has_manifest(dir) {
            manifest = Some(dir.to_path_buf());
        }
    }

    if let Some(path) = workspace {
        return Some(ProjectRoot {
            path,
            kind: RootKind::Workspace,
        });
    }
    manifest.map(|path| ProjectRoot {
        path: lift_tauri_backend(&path, config),
        kind: RootKind::Manifest,
    })
}

/// Canonicalises the deepest existing ancestor and re-appends the rest, so a
/// vanished cwd still maps to the same canonical project identity.
pub fn canonicalize_lenient(path: &Path) -> PathBuf {
    if let Ok(canonical) = fs::canonicalize(path) {
        return canonical;
    }
    let mut missing: Vec<&std::ffi::OsStr> = Vec::new();
    for ancestor in path.ancestors() {
        if let Ok(mut base) = fs::canonicalize(ancestor) {
            for part in missing.iter().rev() {
                base.push(part);
            }
            return base;
        }
        match ancestor.file_name() {
            Some(name) => missing.push(name),
            None => break,
        }
    }
    path.to_path_buf()
}

/// `.git` directory, or `.git` file (worktrees and submodules).
fn is_git_root(dir: &Path) -> bool {
    fs::symlink_metadata(dir.join(".git"))
        .map(|m| m.is_dir() || m.is_file())
        .unwrap_or(false)
}

fn has_manifest(dir: &Path) -> bool {
    MANIFEST_FILES.iter().any(|name| dir.join(name).is_file())
}

fn is_workspace_root(dir: &Path) -> bool {
    if WORKSPACE_FILES.iter().any(|name| dir.join(name).is_file()) {
        return true;
    }
    if let Some(text) = read_small(&dir.join("Cargo.toml")) {
        if let Ok(table) = text.parse::<toml::Table>() {
            if table.contains_key("workspace") {
                return true;
            }
        }
    }
    if let Some(text) = read_small(&dir.join("package.json")) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            if value.get("workspaces").is_some() {
                return true;
            }
        }
    }
    false
}

/// Without Git, a Tauri backend crate (`src-tauri/`) belongs to the app that
/// owns it, not to a project of its own.
fn lift_tauri_backend(path: &Path, config: &DetectorConfig) -> PathBuf {
    let is_backend = path.file_name().is_some_and(|n| n == "src-tauri")
        && (path.join("tauri.conf.json").is_file()
            || path.join("tauri.conf.json5").is_file()
            || path.join("Tauri.toml").is_file());
    if !is_backend {
        return path.to_path_buf();
    }
    match path.parent() {
        Some(parent)
            if parent.parent().is_some()
                && config.home.as_deref() != Some(parent)
                && has_manifest(parent) =>
        {
            parent.to_path_buf()
        }
        _ => path.to_path_buf(),
    }
}

/// Reads a small text file. Oversized/unreadable files are a detection miss.
pub fn read_small(path: &Path) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_MANIFEST_BYTES {
        return None;
    }
    fs::read_to_string(path).ok()
}

struct CacheEntry {
    result: Option<ProjectRoot>,
    checked: Instant,
}

/// Per-cwd cache so ancestor walks are not repeated on every 2 s scan.
pub struct ProjectDetector {
    config: DetectorConfig,
    ttl: Duration,
    cache: HashMap<PathBuf, CacheEntry>,
}

impl ProjectDetector {
    pub fn new(config: DetectorConfig, ttl: Duration) -> Self {
        Self {
            config,
            ttl,
            cache: HashMap::new(),
        }
    }

    pub fn detect(&mut self, cwd: &Path, now: Instant) -> Option<ProjectRoot> {
        if let Some(entry) = self.cache.get(cwd) {
            if now.saturating_duration_since(entry.checked) < self.ttl {
                return entry.result.clone();
            }
        }
        let result = detect_root(cwd, &self.config);
        self.cache.insert(
            cwd.to_path_buf(),
            CacheEntry {
                result: result.clone(),
                checked: now,
            },
        );
        result
    }

    /// Drop entries for working directories no longer in use.
    pub fn retain_seen(&mut self, seen: &HashSet<PathBuf>) {
        self.cache.retain(|cwd, _| seen.contains(cwd));
    }

    pub fn forget_root(&mut self, root: &Path) {
        self.cache
            .retain(|_, entry| entry.result.as_ref().map(|r| r.path.as_path()) != Some(root));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn canon(p: &Path) -> PathBuf {
        fs::canonicalize(p).unwrap()
    }

    fn touch(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn cfg(home: Option<&Path>) -> DetectorConfig {
        DetectorConfig::new(home.map(Path::to_path_buf))
    }

    #[test]
    fn git_directory_defines_the_root() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("app");
        fs::create_dir_all(root.join(".git")).unwrap();
        touch(&root.join("package.json"), "{}");
        fs::create_dir_all(root.join("src/deep")).unwrap();
        let found = detect_root(&root.join("src/deep"), &cfg(None)).unwrap();
        assert_eq!(found.path, canon(&root));
        assert_eq!(found.kind, RootKind::Git);
    }

    #[test]
    fn git_file_marks_a_worktree_root() {
        let tmp = TempDir::new().unwrap();
        let main = tmp.path().join("main");
        fs::create_dir_all(main.join(".git/worktrees/feature")).unwrap();
        let worktree = tmp.path().join("feature-wt");
        touch(
            &worktree.join(".git"),
            &format!("gitdir: {}/.git/worktrees/feature\n", main.display()),
        );
        touch(&worktree.join("package.json"), "{}");
        fs::create_dir_all(worktree.join("packages/x")).unwrap();
        let found = detect_root(&worktree.join("packages/x"), &cfg(None)).unwrap();
        assert_eq!(found.path, canon(&worktree));
        assert_eq!(found.kind, RootKind::Git);
    }

    #[test]
    fn nested_submodule_git_file_is_its_own_root() {
        let tmp = TempDir::new().unwrap();
        let outer = tmp.path().join("outer");
        fs::create_dir_all(outer.join(".git")).unwrap();
        let sub = outer.join("vendor/lib");
        touch(&sub.join(".git"), "gitdir: ../../.git/modules/lib\n");
        fs::create_dir_all(sub.join("src")).unwrap();
        let inside_sub = detect_root(&sub.join("src"), &cfg(None)).unwrap();
        assert_eq!(inside_sub.path, canon(&sub));
        let outside_sub = detect_root(&outer.join("vendor"), &cfg(None)).unwrap();
        assert_eq!(outside_sub.path, canon(&outer));
    }

    #[test]
    fn monorepo_packages_resolve_to_the_git_root() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("mono");
        fs::create_dir_all(root.join(".git")).unwrap();
        touch(&root.join("pnpm-workspace.yaml"), "packages: ['apps/*']\n");
        touch(&root.join("apps/web/package.json"), "{}");
        touch(&root.join("apps/api/package.json"), "{}");
        let web = detect_root(&root.join("apps/web"), &cfg(None)).unwrap();
        let api = detect_root(&root.join("apps/api"), &cfg(None)).unwrap();
        assert_eq!(web.path, canon(&root));
        assert_eq!(web, api);
    }

    #[test]
    fn workspace_root_wins_over_nearer_manifest_without_git() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("mono");
        touch(&root.join("turbo.json"), "{}");
        touch(&root.join("package.json"), "{}");
        touch(&root.join("apps/web/package.json"), "{}");
        let found = detect_root(&root.join("apps/web"), &cfg(None)).unwrap();
        assert_eq!(found.path, canon(&root));
        assert_eq!(found.kind, RootKind::Workspace);
    }

    #[test]
    fn recognises_every_workspace_declaration() {
        for (name, content) in [
            ("pnpm-workspace.yaml", ""),
            ("turbo.json", "{}"),
            ("nx.json", "{}"),
            ("lerna.json", "{}"),
            ("go.work", "go 1.22\n"),
            ("Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n"),
            ("package.json", r#"{"workspaces":["packages/*"]}"#),
        ] {
            let tmp = TempDir::new().unwrap();
            let root = tmp.path().join("ws");
            touch(&root.join(name), content);
            touch(&root.join("pkg/package.json"), "{}");
            let found = detect_root(&root.join("pkg"), &cfg(None)).unwrap();
            assert_eq!(found.path, canon(&root), "{name}");
            assert_eq!(found.kind, RootKind::Workspace, "{name}");
        }
    }

    #[test]
    fn nearest_manifest_is_used_when_nothing_stronger_exists() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("solo");
        touch(&root.join("composer.json"), "{}");
        fs::create_dir_all(root.join("public")).unwrap();
        let found = detect_root(&root.join("public"), &cfg(None)).unwrap();
        assert_eq!(found.path, canon(&root));
        assert_eq!(found.kind, RootKind::Manifest);
        for marker in [
            "package.json",
            "Cargo.toml",
            "pyproject.toml",
            "go.mod",
            "docker-compose.yml",
            "compose.yml",
        ] {
            let dir = tmp.path().join(format!("m-{marker}"));
            touch(&dir.join(marker), "");
            assert!(detect_root(&dir, &cfg(None)).is_some(), "{marker}");
        }
    }

    #[test]
    fn tauri_layout_with_git_is_one_project() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("localdeck");
        fs::create_dir_all(root.join(".git")).unwrap();
        touch(&root.join("package.json"), r#"{"name":"localdeck"}"#);
        touch(
            &root.join("src-tauri/Cargo.toml"),
            "[package]\nname=\"localdeck\"\n",
        );
        touch(&root.join("src-tauri/tauri.conf.json"), "{}");
        fs::create_dir_all(root.join("src")).unwrap();
        let frontend = detect_root(&root.join("src"), &cfg(None)).unwrap();
        let backend = detect_root(&root.join("src-tauri"), &cfg(None)).unwrap();
        assert_eq!(frontend, backend);
        assert_eq!(frontend.path, canon(&root));
    }

    #[test]
    fn tauri_layout_without_git_lifts_backend_to_the_app_root() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("app");
        touch(&root.join("package.json"), "{}");
        touch(
            &root.join("src-tauri/Cargo.toml"),
            "[package]\nname=\"app\"\n",
        );
        touch(&root.join("src-tauri/tauri.conf.json"), "{}");
        let backend = detect_root(&root.join("src-tauri"), &cfg(None)).unwrap();
        let frontend = detect_root(&root, &cfg(None)).unwrap();
        assert_eq!(backend.path, canon(&root));
        assert_eq!(frontend.path, canon(&root));
    }

    #[test]
    fn plain_src_tauri_directory_without_tauri_config_is_not_lifted() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("odd");
        touch(&root.join("package.json"), "{}");
        touch(
            &root.join("src-tauri/Cargo.toml"),
            "[package]\nname=\"x\"\n",
        );
        let found = detect_root(&root.join("src-tauri"), &cfg(None)).unwrap();
        assert_eq!(found.path, canon(&root.join("src-tauri")));
    }

    #[test]
    fn no_markers_means_no_project() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("scratch/notes");
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(detect_root(&dir, &cfg(None)), None);
    }

    #[test]
    fn filesystem_root_is_never_a_project() {
        assert_eq!(detect_root(Path::new("/"), &cfg(None)), None);
    }

    #[test]
    fn home_directory_is_never_a_root_even_with_a_dotfiles_repo() {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path().join("home/dev");
        fs::create_dir_all(home.join(".git")).unwrap();
        touch(&home.join("package.json"), "{}");
        fs::create_dir_all(home.join("Downloads")).unwrap();
        assert_eq!(detect_root(&home, &cfg(Some(&home))), None);
        assert_eq!(
            detect_root(&home.join("Downloads"), &cfg(Some(&home))),
            None
        );
        // A real project below home is still found.
        let project = home.join("Projects/site");
        fs::create_dir_all(project.join(".git")).unwrap();
        let found = detect_root(&project, &cfg(Some(&home))).unwrap();
        assert_eq!(found.path, canon(&project));
    }

    #[test]
    fn system_and_package_manager_locations_are_never_projects() {
        // Homebrew's prefix is itself a git checkout; services that run with it
        // as cwd are infrastructure, not a project.
        for path in [
            "/opt/homebrew",
            "/opt/homebrew/var/mysql",
            "/usr/local/Homebrew/Library",
            "/Library/Application Support/x",
        ] {
            assert_eq!(detect_root(Path::new(path), &cfg(None)), None, "{path}");
        }
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("opt/app");
        fs::create_dir_all(project.join(".git")).unwrap();
        assert!(detect_root(&project, &cfg(None)).is_some());
    }

    #[test]
    fn paths_with_spaces_and_non_ascii_are_handled() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("Projeler Klasörü/Anılarım 日本語");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(root.join("web app/src")).unwrap();
        let found = detect_root(&root.join("web app/src"), &cfg(None)).unwrap();
        assert_eq!(found.path, canon(&root));
    }

    #[test]
    fn malformed_manifests_do_not_panic_or_count_as_workspaces() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("broken");
        touch(&root.join("Cargo.toml"), "this is = = not toml [[[");
        touch(&root.join("package.json"), "{ nope");
        let found = detect_root(&root, &cfg(None)).unwrap();
        assert_eq!(found.kind, RootKind::Manifest);
    }

    #[test]
    fn missing_cwd_still_resolves_from_surviving_ancestors() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("proj");
        fs::create_dir_all(root.join(".git")).unwrap();
        let gone = root.join("deleted/dir");
        let found = detect_root(&gone, &cfg(None)).unwrap();
        assert_eq!(found.path, canon(&root));
    }

    #[test]
    fn oversized_manifest_is_a_detection_miss() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("package.json");
        fs::write(&file, vec![b' '; (MAX_MANIFEST_BYTES + 1) as usize]).unwrap();
        assert_eq!(read_small(&file), None);
    }

    #[test]
    fn cache_serves_results_within_ttl_then_revalidates() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("later");
        fs::create_dir_all(&dir).unwrap();
        let mut detector = ProjectDetector::new(cfg(None), Duration::from_secs(30));
        let t0 = Instant::now();
        assert_eq!(detector.detect(&dir, t0), None);
        fs::create_dir_all(dir.join(".git")).unwrap();
        assert_eq!(detector.detect(&dir, t0 + Duration::from_secs(5)), None);
        let found = detector.detect(&dir, t0 + Duration::from_secs(31)).unwrap();
        assert_eq!(found.path, canon(&dir));
    }

    #[test]
    fn forget_root_and_retain_seen_evict_entries() {
        let tmp = TempDir::new().unwrap();
        let a = tmp.path().join("a");
        let b = tmp.path().join("b");
        fs::create_dir_all(a.join(".git")).unwrap();
        fs::create_dir_all(b.join(".git")).unwrap();
        let mut detector = ProjectDetector::new(cfg(None), Duration::from_secs(3600));
        let t0 = Instant::now();
        detector.detect(&a, t0);
        detector.detect(&b, t0);
        detector.forget_root(&canon(&a));
        assert_eq!(detector.cache.len(), 1);
        detector.retain_seen(&HashSet::new());
        assert!(detector.cache.is_empty());
    }
}
