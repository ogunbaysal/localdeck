//! Stack fingerprinting from a handful of small, well-known project files.
//!
//! Never walks the repository. Results are cached per root and revalidated by
//! comparing a cheap metadata signature of the marker files (see [`StackCache`]).

use super::project_detector::read_small;
use crate::models::{PackageManager, PackageScript, StackTag};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StackFingerprint {
    /// Humanised project name (package.json → Cargo → composer → pyproject → folder).
    pub detected_name: String,
    pub stack: Vec<StackTag>,
    /// `Some` only when the project has a `package.json`.
    pub package_manager: Option<PackageManager>,
    /// False when the manager is only the npm default (no lockfile/field).
    pub package_manager_detected: bool,
    pub scripts: Vec<PackageScript>,
}

impl StackFingerprint {
    pub fn has(&self, id: &str) -> bool {
        self.stack.iter().any(|t| t.id == id)
    }

    pub fn script(&self, name: &str) -> Option<&PackageScript> {
        self.scripts.iter().find(|s| s.name == name)
    }
}

const TAGS: &[(&str, &str)] = &[
    ("nextjs", "Next.js"),
    ("nuxt", "Nuxt"),
    ("astro", "Astro"),
    ("remix", "Remix"),
    ("sveltekit", "SvelteKit"),
    ("laravel", "Laravel"),
    ("django", "Django"),
    ("fastapi", "FastAPI"),
    ("flask", "Flask"),
    ("nestjs", "NestJS"),
    ("tauri", "Tauri"),
    ("react", "React"),
    ("vue", "Vue"),
    ("svelte", "Svelte"),
    ("vite", "Vite"),
    ("bun", "Bun"),
    ("node", "Node.js"),
    ("deno", "Deno"),
    ("typescript", "TypeScript"),
    ("rust", "Rust"),
    ("php", "PHP"),
    ("python", "Python"),
    ("go", "Go"),
    ("docker", "Docker"),
];

const VITE_CONFIGS: &[&str] = &[
    "vite.config.ts",
    "vite.config.js",
    "vite.config.mjs",
    "vite.config.cjs",
    "vite.config.mts",
];
const NEXT_CONFIGS: &[&str] = &["next.config.js", "next.config.mjs", "next.config.ts"];
const NUXT_CONFIGS: &[&str] = &["nuxt.config.ts", "nuxt.config.js", "nuxt.config.mjs"];
const ASTRO_CONFIGS: &[&str] = &["astro.config.mjs", "astro.config.ts", "astro.config.js"];
const SVELTE_CONFIGS: &[&str] = &["svelte.config.js", "svelte.config.ts"];
const COMPOSE_FILES: &[&str] = &[
    "docker-compose.yml",
    "docker-compose.yaml",
    "compose.yml",
    "compose.yaml",
];

/// Files whose metadata decides whether a cached fingerprint is still valid.
const SIGNATURE_FILES: &[&str] = &[
    "package.json",
    "bun.lock",
    "bun.lockb",
    "bunfig.toml",
    "pnpm-lock.yaml",
    "yarn.lock",
    "package-lock.json",
    "Cargo.toml",
    "composer.json",
    "pyproject.toml",
    "go.mod",
    "go.work",
    "artisan",
    "tsconfig.json",
    "deno.json",
    "Dockerfile",
    "docker-compose.yml",
    "docker-compose.yaml",
    "compose.yml",
    "compose.yaml",
    "src-tauri",
    "src-tauri/tauri.conf.json",
    "vite.config.ts",
    "vite.config.js",
    "vite.config.mjs",
    "vite.config.cjs",
    "vite.config.mts",
    "next.config.js",
    "next.config.mjs",
    "next.config.ts",
    "nuxt.config.ts",
    "nuxt.config.js",
    "nuxt.config.mjs",
    "astro.config.mjs",
    "astro.config.ts",
    "astro.config.js",
    "svelte.config.js",
    "svelte.config.ts",
];

fn any_exists(root: &Path, names: &[&str]) -> bool {
    names.iter().any(|n| root.join(n).exists())
}

/// Humanise a package-style name for display (`@scope/my-app` → `My App`).
/// Names that already contain capitals or spaces are left alone.
pub fn humanize_name(raw: &str) -> String {
    let base = raw.rsplit('/').next().unwrap_or(raw).trim();
    if base.is_empty() {
        return String::new();
    }
    if base.chars().any(|c| c.is_uppercase() || c.is_whitespace()) || !base.is_ascii() {
        return base.to_string();
    }
    base.split(['-', '_', '.'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn folder_name(root: &Path) -> String {
    root.file_name()
        .map(|n| humanize_name(&n.to_string_lossy()))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| root.to_string_lossy().to_string())
}

struct PackageJson {
    name: Option<String>,
    deps: HashSet<String>,
    scripts: Vec<PackageScript>,
    package_manager_field: Option<String>,
}

fn read_package_json(root: &Path) -> Option<PackageJson> {
    let text = read_small(&root.join("package.json"))?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let object = value.as_object()?;
    let mut deps = HashSet::new();
    for key in [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ] {
        if let Some(map) = object.get(key).and_then(|v| v.as_object()) {
            deps.extend(map.keys().cloned());
        }
    }
    let scripts = object
        .get("scripts")
        .and_then(|v| v.as_object())
        .map(|map| {
            map.iter()
                .filter_map(|(name, cmd)| {
                    cmd.as_str().map(|c| PackageScript {
                        name: name.clone(),
                        command: c.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(PackageJson {
        name: object
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        deps,
        scripts,
        package_manager_field: object
            .get("packageManager")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    })
}

/// `packageManager` field first, then lockfiles in a fixed order.
pub fn detect_package_manager(
    root: &Path,
    package_manager_field: Option<&str>,
) -> (PackageManager, bool) {
    if let Some(field) = package_manager_field {
        let name = field.split('@').next().unwrap_or("");
        match name {
            "bun" => return (PackageManager::Bun, true),
            "pnpm" => return (PackageManager::Pnpm, true),
            "yarn" => return (PackageManager::Yarn, true),
            "npm" => return (PackageManager::Npm, true),
            _ => {}
        }
    }
    if any_exists(root, &["bun.lock", "bun.lockb"]) {
        (PackageManager::Bun, true)
    } else if root.join("pnpm-lock.yaml").exists() {
        (PackageManager::Pnpm, true)
    } else if root.join("yarn.lock").exists() {
        (PackageManager::Yarn, true)
    } else if root.join("package-lock.json").exists() {
        (PackageManager::Npm, true)
    } else {
        (PackageManager::Npm, false)
    }
}

/// Order scripts so likely start commands come first; the rest keep file order.
fn order_scripts(mut scripts: Vec<PackageScript>) -> Vec<PackageScript> {
    const PREFERRED: &[&str] = &["dev", "start", "serve", "develop", "preview"];
    scripts.sort_by_key(|s| {
        PREFERRED
            .iter()
            .position(|p| *p == s.name)
            .unwrap_or(PREFERRED.len())
    });
    scripts
}

struct CargoInfo {
    name: Option<String>,
}

fn read_cargo(root: &Path) -> Option<CargoInfo> {
    let text = read_small(&root.join("Cargo.toml"))?;
    let table: toml::Table = text.parse().ok()?;
    let name = table
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .or_else(|| {
            table
                .get("workspace")
                .and_then(|w| w.get("package"))
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
        })
        .map(str::to_string);
    Some(CargoInfo { name })
}

fn read_composer(root: &Path) -> Option<(Option<String>, bool)> {
    let text = read_small(&root.join("composer.json"))?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let laravel = ["require", "require-dev"].iter().any(|key| {
        value
            .get(key)
            .and_then(|v| v.as_object())
            .is_some_and(|m| m.contains_key("laravel/framework"))
    });
    Some((name, laravel))
}

struct PyProject {
    name: Option<String>,
    frameworks: Vec<&'static str>,
}

fn read_pyproject(root: &Path) -> Option<PyProject> {
    let text = read_small(&root.join("pyproject.toml"))?;
    let table: toml::Table = text.parse().ok()?;
    let name = table
        .get("project")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .or_else(|| {
            table
                .get("tool")
                .and_then(|t| t.get("poetry"))
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
        })
        .map(str::to_string);
    let mut dependency_text = String::new();
    if let Some(list) = table
        .get("project")
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_array())
    {
        for dep in list.iter().filter_map(|d| d.as_str()) {
            dependency_text.push_str(&dep.to_lowercase());
            dependency_text.push('\n');
        }
    }
    if let Some(map) = table
        .get("tool")
        .and_then(|t| t.get("poetry"))
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_table())
    {
        for key in map.keys() {
            dependency_text.push_str(&key.to_lowercase());
            dependency_text.push('\n');
        }
    }
    let mut frameworks = Vec::new();
    for (needle, id) in [
        ("django", "django"),
        ("fastapi", "fastapi"),
        ("flask", "flask"),
    ] {
        if dependency_text
            .lines()
            .any(|l| l.trim_start().starts_with(needle))
        {
            frameworks.push(id);
        }
    }
    Some(PyProject { name, frameworks })
}

/// Fingerprint a project root. Never fails: missing or broken files are a miss.
pub fn detect_stack(root: &Path) -> StackFingerprint {
    let mut ids: HashSet<&'static str> = HashSet::new();
    let mut name: Option<String> = None;
    let mut scripts = Vec::new();
    let mut package_manager = None;
    let mut package_manager_detected = false;

    let package = read_package_json(root);
    let has_package_json = root.join("package.json").is_file();
    if has_package_json {
        let (manager, detected) = detect_package_manager(
            root,
            package
                .as_ref()
                .and_then(|p| p.package_manager_field.as_deref()),
        );
        package_manager = Some(manager);
        package_manager_detected = detected;
    }
    if let Some(package) = &package {
        name = package.name.clone().filter(|n| !n.trim().is_empty());
        scripts = order_scripts(package.scripts.clone());
        let dep = |n: &str| package.deps.contains(n);
        let dep_prefix = |p: &str| package.deps.iter().any(|d| d.starts_with(p));
        if dep("next") {
            ids.insert("nextjs");
        }
        if dep("nuxt") || dep("nuxt3") {
            ids.insert("nuxt");
        }
        if dep("astro") {
            ids.insert("astro");
        }
        if dep_prefix("@remix-run/") {
            ids.insert("remix");
        }
        if dep("@sveltejs/kit") {
            ids.insert("sveltekit");
        }
        if dep("svelte") {
            ids.insert("svelte");
        }
        if dep("vue") {
            ids.insert("vue");
        }
        if dep("react") || dep("react-dom") {
            ids.insert("react");
        }
        if dep("vite") {
            ids.insert("vite");
        }
        if dep("@nestjs/core") {
            ids.insert("nestjs");
        }
        if dep("@tauri-apps/cli") || dep("@tauri-apps/api") || dep_prefix("@tauri-apps/plugin-") {
            ids.insert("tauri");
        }
        if dep("typescript") {
            ids.insert("typescript");
        }
    }

    if any_exists(root, VITE_CONFIGS) {
        ids.insert("vite");
    }
    if any_exists(root, NEXT_CONFIGS) {
        ids.insert("nextjs");
    }
    if any_exists(root, NUXT_CONFIGS) {
        ids.insert("nuxt");
    }
    if any_exists(root, ASTRO_CONFIGS) {
        ids.insert("astro");
    }
    if any_exists(root, SVELTE_CONFIGS) {
        ids.insert("svelte");
    }
    if root.join("src-tauri").is_dir() {
        ids.insert("tauri");
    }
    if root.join("tsconfig.json").is_file() {
        ids.insert("typescript");
    }

    // Runtime
    if any_exists(root, &["bun.lock", "bun.lockb", "bunfig.toml"])
        || package_manager == Some(PackageManager::Bun) && package_manager_detected
    {
        ids.insert("bun");
    } else if any_exists(root, &["deno.json", "deno.jsonc"]) {
        ids.insert("deno");
    } else if has_package_json {
        ids.insert("node");
    }

    // Rust
    let cargo = read_cargo(root);
    if root.join("Cargo.toml").is_file() || root.join("src-tauri/Cargo.toml").is_file() {
        ids.insert("rust");
    }
    if let Some(cargo) = &cargo {
        if name.is_none() {
            name = cargo.name.clone().filter(|n| !n.trim().is_empty());
        }
    }

    // PHP
    if let Some((composer_name, laravel)) = read_composer(root) {
        ids.insert("php");
        if laravel {
            ids.insert("laravel");
        }
        if name.is_none() {
            name = composer_name.filter(|n| !n.trim().is_empty());
        }
    } else if root.join("composer.json").is_file() {
        ids.insert("php");
    }
    if root.join("artisan").is_file() {
        ids.insert("php");
        ids.insert("laravel");
    }

    // Python
    if root.join("pyproject.toml").is_file() {
        ids.insert("python");
        if let Some(py) = read_pyproject(root) {
            for framework in py.frameworks {
                ids.insert(framework);
            }
            if name.is_none() {
                name = py.name.filter(|n| !n.trim().is_empty());
            }
        }
    }

    // Go
    if any_exists(root, &["go.mod", "go.work"]) {
        ids.insert("go");
    }

    // Docker
    if any_exists(root, COMPOSE_FILES) || root.join("Dockerfile").is_file() {
        ids.insert("docker");
    }

    let stack = TAGS
        .iter()
        .filter(|(id, _)| ids.contains(id))
        .map(|(id, label)| StackTag {
            id: (*id).to_string(),
            label: (*label).to_string(),
        })
        .collect();

    StackFingerprint {
        detected_name: name
            .map(|n| humanize_name(&n))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| folder_name(root)),
        stack,
        package_manager,
        package_manager_detected,
        scripts,
    }
}

type Signature = Vec<Option<(u128, u64)>>;

fn signature(root: &Path) -> Signature {
    SIGNATURE_FILES
        .iter()
        .map(|name| {
            fs::metadata(root.join(name)).ok().map(|m| {
                let modified = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                (modified, m.len())
            })
        })
        .collect()
}

struct CacheEntry {
    fingerprint: Arc<StackFingerprint>,
    signature: Signature,
    checked: Instant,
}

/// Fingerprint cache: reparses only when marker metadata changed, and checks
/// that at most once per `ttl` per project.
#[derive(Default)]
pub struct StackCache {
    entries: HashMap<PathBuf, CacheEntry>,
}

impl StackCache {
    pub fn get(&mut self, root: &Path, now: Instant, ttl: Duration) -> Arc<StackFingerprint> {
        if let Some(entry) = self.entries.get_mut(root) {
            if now.saturating_duration_since(entry.checked) < ttl {
                return entry.fingerprint.clone();
            }
            let current = signature(root);
            entry.checked = now;
            if current == entry.signature {
                return entry.fingerprint.clone();
            }
            let fingerprint = Arc::new(detect_stack(root));
            entry.fingerprint = fingerprint.clone();
            entry.signature = current;
            return fingerprint;
        }
        let fingerprint = Arc::new(detect_stack(root));
        self.entries.insert(
            root.to_path_buf(),
            CacheEntry {
                fingerprint: fingerprint.clone(),
                signature: signature(root),
                checked: now,
            },
        );
        fingerprint
    }

    /// User-requested refresh.
    pub fn invalidate(&mut self, root: &Path) {
        self.entries.remove(root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn labels(fp: &StackFingerprint) -> Vec<&str> {
        fp.stack.iter().map(|t| t.label.as_str()).collect()
    }

    #[test]
    fn detects_a_tauri_react_vite_bun_project() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(
            root,
            "package.json",
            r#"{"name":"localdeck","scripts":{"build":"tsc && vite build","dev":"vite","tauri":"tauri"},
               "dependencies":{"react":"^19","@tauri-apps/api":"^2"},
               "devDependencies":{"vite":"^8","typescript":"~6","@tauri-apps/cli":"^2"}}"#,
        );
        write(root, "bun.lock", "");
        write(
            root,
            "src-tauri/Cargo.toml",
            "[package]\nname=\"localdeck\"\n",
        );
        write(root, "vite.config.ts", "");
        write(root, "tsconfig.json", "{}");
        let fp = detect_stack(root);
        assert_eq!(
            labels(&fp),
            vec!["Tauri", "React", "Vite", "Bun", "TypeScript", "Rust"]
        );
        assert_eq!(fp.detected_name, "Localdeck");
        assert_eq!(fp.package_manager, Some(PackageManager::Bun));
        assert!(fp.package_manager_detected);
    }

    #[test]
    fn detects_next_node_project_with_npm_default() {
        let tmp = TempDir::new().unwrap();
        write(
            tmp.path(),
            "package.json",
            r#"{"name":"@acme/shop-front","dependencies":{"next":"14","react":"18"}}"#,
        );
        let fp = detect_stack(tmp.path());
        assert_eq!(labels(&fp), vec!["Next.js", "React", "Node.js"]);
        assert_eq!(fp.detected_name, "Shop Front");
        assert_eq!(fp.package_manager, Some(PackageManager::Npm));
        assert!(!fp.package_manager_detected);
    }

    #[test]
    fn detects_other_ecosystems() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(
            root,
            "composer.json",
            r#"{"name":"acme/blog","require":{"laravel/framework":"^11"}}"#,
        );
        write(root, "artisan", "#!/usr/bin/env php");
        write(root, "compose.yml", "services: {}");
        assert_eq!(
            labels(&detect_stack(root)),
            vec!["Laravel", "PHP", "Docker"]
        );
        assert_eq!(detect_stack(root).detected_name, "Blog");

        let py = TempDir::new().unwrap();
        write(
            py.path(),
            "pyproject.toml",
            "[project]\nname = \"api-service\"\ndependencies = [\"fastapi>=0.110\", \"uvicorn\"]\n",
        );
        let fp = detect_stack(py.path());
        assert_eq!(labels(&fp), vec!["FastAPI", "Python"]);
        assert_eq!(fp.detected_name, "Api Service");

        let go = TempDir::new().unwrap();
        write(go.path(), "go.mod", "module example.com/x\n");
        assert_eq!(labels(&detect_stack(go.path())), vec!["Go"]);

        let rs = TempDir::new().unwrap();
        write(rs.path(), "Cargo.toml", "[package]\nname=\"my_tool\"\n");
        let fp = detect_stack(rs.path());
        assert_eq!(labels(&fp), vec!["Rust"]);
        assert_eq!(fp.detected_name, "My Tool");
    }

    #[test]
    fn framework_config_files_count_even_without_dependencies() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "astro.config.mjs", "");
        write(tmp.path(), "nuxt.config.ts", "");
        assert_eq!(labels(&detect_stack(tmp.path())), vec!["Nuxt", "Astro"]);
    }

    #[test]
    fn name_falls_back_to_folder_basename_including_non_ascii() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("Anılarım Projesi");
        fs::create_dir_all(&dir).unwrap();
        let fp = detect_stack(&dir);
        assert_eq!(fp.detected_name, "Anılarım Projesi");
        assert!(fp.stack.is_empty());
    }

    #[test]
    fn name_priority_is_package_then_cargo_then_composer_then_python() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(root, "Cargo.toml", "[package]\nname=\"cargo-name\"\n");
        write(root, "composer.json", r#"{"name":"v/composer-name"}"#);
        write(root, "pyproject.toml", "[project]\nname=\"py-name\"\n");
        assert_eq!(detect_stack(root).detected_name, "Cargo Name");
        fs::remove_file(root.join("Cargo.toml")).unwrap();
        assert_eq!(detect_stack(root).detected_name, "Composer Name");
        fs::remove_file(root.join("composer.json")).unwrap();
        assert_eq!(detect_stack(root).detected_name, "Py Name");
        write(root, "package.json", r#"{"name":"pkg-name"}"#);
        assert_eq!(detect_stack(root).detected_name, "Pkg Name");
    }

    #[test]
    fn humanize_leaves_already_human_names_alone() {
        assert_eq!(humanize_name("LocalDeck"), "LocalDeck");
        assert_eq!(humanize_name("XON CRM"), "XON CRM");
        assert_eq!(humanize_name("my-cool_app"), "My Cool App");
        assert_eq!(humanize_name("@scope/pkg-name"), "Pkg Name");
        assert_eq!(humanize_name(""), "");
    }

    #[test]
    fn package_manager_from_lockfiles_and_field() {
        for (file, expected) in [
            ("bun.lock", PackageManager::Bun),
            ("bun.lockb", PackageManager::Bun),
            ("pnpm-lock.yaml", PackageManager::Pnpm),
            ("yarn.lock", PackageManager::Yarn),
            ("package-lock.json", PackageManager::Npm),
        ] {
            let tmp = TempDir::new().unwrap();
            write(tmp.path(), file, "");
            let (manager, detected) = detect_package_manager(tmp.path(), None);
            assert_eq!(manager, expected, "{file}");
            assert!(detected);
        }
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "yarn.lock", "");
        let (manager, _) = detect_package_manager(tmp.path(), Some("pnpm@9.1.0"));
        assert_eq!(manager, PackageManager::Pnpm, "packageManager field wins");
        let (manager, detected) = detect_package_manager(TempDir::new().unwrap().path(), None);
        assert_eq!((manager, detected), (PackageManager::Npm, false));
    }

    #[test]
    fn scripts_are_discovered_with_start_commands_first() {
        let tmp = TempDir::new().unwrap();
        write(
            tmp.path(),
            "package.json",
            r#"{"scripts":{"build":"vite build","lint":"eslint .","dev":"vite","test":"bun test"}}"#,
        );
        let fp = detect_stack(tmp.path());
        let names: Vec<&str> = fp.scripts.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["dev", "build", "lint", "test"]);
        assert_eq!(fp.script("dev").unwrap().command, "vite");
    }

    #[test]
    fn broken_manifests_are_misses_not_crashes() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "package.json", "{ not json");
        write(tmp.path(), "Cargo.toml", "[[[");
        write(tmp.path(), "pyproject.toml", "= =");
        let fp = detect_stack(tmp.path());
        assert!(fp.scripts.is_empty());
        assert!(fp.has("rust"));
    }

    #[test]
    fn cache_returns_the_same_fingerprint_until_markers_change() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(
            root,
            "package.json",
            r#"{"name":"one","dependencies":{"react":"1"}}"#,
        );
        let mut cache = StackCache::default();
        let t0 = Instant::now();
        let ttl = Duration::from_secs(30);
        let first = cache.get(root, t0, ttl);
        // Within ttl: no revalidation even though the file changed.
        write(
            root,
            "package.json",
            r#"{"name":"two","dependencies":{"vue":"1"}}"#,
        );
        let within = cache.get(root, t0 + Duration::from_secs(5), ttl);
        assert!(Arc::ptr_eq(&first, &within));
        // After ttl the signature differs → re-detected.
        let after = cache.get(root, t0 + Duration::from_secs(31), ttl);
        assert!(!Arc::ptr_eq(&first, &after));
        assert_eq!(after.detected_name, "Two");
        assert!(after.has("vue"));
        // Unchanged markers after ttl → same Arc, no reparse.
        let unchanged = cache.get(root, t0 + Duration::from_secs(70), ttl);
        assert!(Arc::ptr_eq(&after, &unchanged));
        // Explicit refresh.
        cache.invalidate(root);
        let refreshed = cache.get(root, t0 + Duration::from_secs(71), ttl);
        assert!(!Arc::ptr_eq(&after, &refreshed));
    }
}
