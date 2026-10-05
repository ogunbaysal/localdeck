//! Start-command discovery from small, well-known project files.
//!
//! Every option resolves to `program + args[] + cwd` — never a shell string.
//! Option ids are stable (`pkg:dev`, `make:serve`, `just:dev`, `composer:serve`,
//! `cargo:run`, `compose:up`, `py:dev`) and are the *only* thing the frontend or
//! an imported file may name; the spawn spec is always re-derived here.

use super::project_detector::read_small;
use super::stack_detector::StackFingerprint;
use crate::models::{PackageManager, PrimaryCommand, StartOption, StartSource};
use std::path::Path;

const MAX_NAME_LEN: usize = 64;
const MAX_PER_SOURCE: usize = 50;

const MAKEFILES: &[&str] = &["Makefile", "GNUmakefile", "makefile"];
const JUSTFILES: &[&str] = &["justfile", "Justfile", ".justfile"];
const COMPOSE_FILES: &[&str] = &[
    "docker-compose.yml",
    "docker-compose.yaml",
    "compose.yml",
    "compose.yaml",
];

/// An option together with how to spawn it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionSpec {
    pub option: StartOption,
    pub program: String,
    pub args: Vec<String>,
}

/// Names must be conservative: they become argv entries.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME_LEN
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-'))
}

fn spec(
    source: StartSource,
    id: String,
    name: &str,
    program: &str,
    args: &[&str],
    detail: Option<String>,
) -> OptionSpec {
    let args: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
    let mut label = program.to_string();
    for arg in &args {
        label.push(' ');
        label.push_str(arg);
    }
    OptionSpec {
        option: StartOption {
            id,
            source,
            name: name.to_string(),
            label,
            detail,
        },
        program: program.to_string(),
        args,
    }
}

fn first_existing(root: &Path, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| read_small(&root.join(name)))
}

/// Makefile targets that can be run as plain `make <target>`.
pub fn parse_make_targets(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        if line.is_empty() || line.starts_with(['\t', ' ', '#']) {
            continue;
        }
        let Some(colon) = line.find(':') else {
            continue;
        };
        let (left, right) = line.split_at(colon);
        // `:=`, `::=` assignments and double-colon rules are not plain targets.
        if right.starts_with(":=") || right.starts_with("::") {
            continue;
        }
        if left.contains(['=', '%', '$', '(', '\\']) {
            continue;
        }
        for token in left.split_whitespace() {
            if token.starts_with('.') || !valid_name(token) || out.iter().any(|t| t == token) {
                continue;
            }
            out.push(token.to_string());
        }
    }
    out
}

/// Parameterless, public `just` recipes.
pub fn parse_just_recipes(text: &str) -> Vec<(String, Option<String>)> {
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    let mut private_next = false;
    let mut doc: Option<String> = None;
    for line in text.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            doc = None;
            private_next = false;
            continue;
        }
        if let Some(comment) = trimmed.strip_prefix('#') {
            doc = Some(comment.trim().to_string());
            continue;
        }
        if trimmed.starts_with([' ', '\t']) {
            continue;
        }
        if trimmed.starts_with('[') {
            let lower = trimmed.to_lowercase();
            if lower.contains("private") {
                private_next = true;
            }
            continue;
        }
        if trimmed.contains(":=") && !trimmed.contains(": ") {
            doc = None;
            continue;
        }
        let Some(colon) = trimmed.find(':') else {
            continue;
        };
        let header = trimmed[..colon].trim_start_matches('@');
        if trimmed[colon..].starts_with(":=") {
            continue;
        }
        let mut parts = header.split_whitespace();
        let Some(name) = parts.next() else {
            continue;
        };
        let first_ok = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
        // A parameter without a default can't be started with no input.
        let has_required_param =
            parts.any(|p| !p.contains('=') && !p.starts_with(['+', '*']) || p.starts_with(['+']));
        if matches!(name, "set" | "alias" | "export" | "import" | "mod")
            || !first_ok
            || has_required_param
            || private_next
            || !valid_name(name)
            || out.iter().any(|(n, _)| n == name)
        {
            private_next = false;
            doc = None;
            continue;
        }
        out.push((name.to_string(), doc.take().filter(|d| !d.is_empty())));
        private_next = false;
    }
    out
}

/// Keys of `composer.json`'s `scripts` object.
fn composer_scripts(text: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    value
        .get("scripts")
        .and_then(|s| s.as_object())
        .map(|map| {
            map.keys()
                .filter(|k| valid_name(k) && !k.starts_with("pre-") && !k.starts_with("post-"))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

fn cargo_has_binary(root: &Path, text: &str) -> bool {
    let Ok(table) = text.parse::<toml::Table>() else {
        return false;
    };
    if !table.contains_key("package") {
        return false;
    }
    root.join("src/main.rs").is_file()
        || table
            .get("bin")
            .and_then(|b| b.as_array())
            .is_some_and(|bins| !bins.is_empty())
}

/// `[project.scripts]` / `[tool.poetry.scripts]` names.
fn pyproject_scripts(text: &str) -> (Vec<String>, bool) {
    let Ok(table) = text.parse::<toml::Table>() else {
        return (Vec::new(), false);
    };
    let mut names: Vec<String> = Vec::new();
    let mut add = |value: Option<&toml::Value>| {
        if let Some(map) = value.and_then(|v| v.as_table()) {
            for key in map.keys() {
                if valid_name(key) && !names.contains(key) {
                    names.push(key.clone());
                }
            }
        }
    };
    add(table.get("project").and_then(|p| p.get("scripts")));
    let poetry = table.get("tool").and_then(|t| t.get("poetry"));
    add(poetry.and_then(|p| p.get("scripts")));
    (names, poetry.is_some())
}

/// All start options for a project, in a stable order: package scripts
/// (start-like names first), make, just, composer, cargo, compose, python.
pub fn discover(root: &Path, fingerprint: &StackFingerprint) -> Vec<OptionSpec> {
    let mut out: Vec<OptionSpec> = Vec::new();

    if let Some(manager) = fingerprint.package_manager {
        for script in fingerprint
            .scripts
            .iter()
            .filter(|s| valid_name(&s.name))
            .take(MAX_PER_SOURCE)
        {
            out.push(spec(
                StartSource::PackageJson,
                format!("pkg:{}", script.name),
                &script.name,
                manager.program(),
                &["run", &script.name],
                Some(script.command.clone()),
            ));
        }
    }

    if let Some(text) = first_existing(root, MAKEFILES) {
        for target in parse_make_targets(&text).into_iter().take(MAX_PER_SOURCE) {
            out.push(spec(
                StartSource::Makefile,
                format!("make:{target}"),
                &target,
                "make",
                &[&target],
                None,
            ));
        }
    }

    if let Some(text) = first_existing(root, JUSTFILES) {
        for (recipe, doc) in parse_just_recipes(&text).into_iter().take(MAX_PER_SOURCE) {
            out.push(spec(
                StartSource::Just,
                format!("just:{recipe}"),
                &recipe,
                "just",
                &[&recipe],
                doc,
            ));
        }
    }

    if let Some(text) = read_small(&root.join("composer.json")) {
        for script in composer_scripts(&text).into_iter().take(MAX_PER_SOURCE) {
            out.push(spec(
                StartSource::Composer,
                format!("composer:{script}"),
                &script,
                "composer",
                &["run-script", &script],
                None,
            ));
        }
    }

    if let Some(text) = read_small(&root.join("Cargo.toml")) {
        if cargo_has_binary(root, &text) {
            out.push(spec(
                StartSource::Cargo,
                "cargo:run".to_string(),
                "run",
                "cargo",
                &["run"],
                None,
            ));
        }
    }

    if COMPOSE_FILES.iter().any(|f| root.join(f).is_file()) {
        out.push(spec(
            StartSource::Compose,
            "compose:up".to_string(),
            "up",
            "docker",
            &["compose", "up"],
            None,
        ));
    }

    if let Some(text) = read_small(&root.join("pyproject.toml")) {
        let (scripts, poetry) = pyproject_scripts(&text);
        let runner = if root.join("uv.lock").is_file() {
            Some("uv")
        } else if poetry || root.join("poetry.lock").is_file() {
            Some("poetry")
        } else {
            None
        };
        if let Some(runner) = runner {
            for script in scripts.into_iter().take(MAX_PER_SOURCE) {
                out.push(spec(
                    StartSource::Pyproject,
                    format!("py:{script}"),
                    &script,
                    runner,
                    &["run", &script],
                    None,
                ));
            }
        }
    }
    out
}

/// Resolve one option id against the project's current files.
pub fn resolve(root: &Path, fingerprint: &StackFingerprint, id: &str) -> Option<OptionSpec> {
    // Accept a bare package-script name for older stored values.
    let wanted = if id.contains(':') {
        id.to_string()
    } else {
        format!("pkg:{id}")
    };
    discover(root, fingerprint)
        .into_iter()
        .find(|s| s.option.id == wanted)
}

/// The persisted representation of an option.
pub fn to_command(root: &Path, spec: &OptionSpec) -> PrimaryCommand {
    PrimaryCommand {
        label: spec.option.label.clone(),
        program: spec.program.clone(),
        args: spec.args.clone(),
        cwd: root.to_string_lossy().to_string(),
        script: Some(spec.option.id.clone()),
    }
}

/// Dev servers that accept `--port`, keyed by stack id.
const PORT_FLAG_STACKS: &[&str] = &["vite", "astro", "nextjs", "nuxt", "sveltekit"];

/// Extra args/env so a command listens on `port`: always `PORT`, plus a
/// manager-correct `--port <n>` for package scripts of known dev servers.
pub fn with_port(
    command: &PrimaryCommand,
    fingerprint: &StackFingerprint,
    port: u16,
) -> (PrimaryCommand, Vec<(String, String)>) {
    let mut command = command.clone();
    let is_pkg = command
        .script
        .as_deref()
        .is_some_and(|s| s.starts_with("pkg:"));
    let known = PORT_FLAG_STACKS.iter().any(|id| fingerprint.has(id));
    if is_pkg && known {
        if matches!(
            fingerprint.package_manager,
            Some(PackageManager::Npm) | None
        ) {
            command.args.push("--".to_string());
        }
        command.args.push("--port".to_string());
        command.args.push(port.to_string());
        command.label = format!("{} (port {port})", command.label);
    }
    (command, vec![("PORT".to_string(), port.to_string())])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::stack_detector::detect_stack;
    use std::fs;
    use tempfile::TempDir;

    fn write(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn ids(root: &Path) -> Vec<String> {
        discover(root, &detect_stack(root))
            .into_iter()
            .map(|s| s.option.id)
            .collect()
    }

    #[test]
    fn names_are_conservative() {
        for ok in ["dev", "build:prod", "a.b-c_d", "serve2"] {
            assert!(valid_name(ok), "{ok}");
        }
        for bad in [
            "",
            "-rf",
            "--port",
            "a b",
            "a;b",
            "a/b",
            "$(x)",
            "é",
            &"x".repeat(65),
        ] {
            assert!(!valid_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn makefile_targets_skip_specials_patterns_and_assignments() {
        let text = "\
.PHONY: all serve
CC := gcc
FLAGS = -O2: fast
all: build
%.o: %.c
\t$(CC) -c $<
serve dev: deps
\tpython -m http.server
build:: x
# comment: nope
ifeq ($(X),y)
endif
-bad: x
";
        assert_eq!(parse_make_targets(text), vec!["all", "serve", "dev"]);
    }

    #[test]
    fn just_recipes_skip_private_assignments_and_required_params() {
        let text = "\
set shell := [\"bash\", \"-c\"]
version := \"1\"

# start the dev server
dev:
    bun run dev

_helper:
    echo hi

[private]
hidden:
    echo no

deploy env:
    ./deploy {{env}}

serve port='8000':
    python -m http.server {{port}}

alias d := dev
";
        let recipes = parse_just_recipes(text);
        let names: Vec<&str> = recipes.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["dev", "serve"]);
        assert_eq!(recipes[0].1.as_deref(), Some("start the dev server"));
    }

    #[test]
    fn discovers_every_source_in_stable_order_with_spawn_specs() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("Çok projeler");
        write(
            &root,
            "package.json",
            r#"{"scripts":{"build":"x","dev":"vite"}}"#,
        );
        write(&root, "bun.lock", "");
        write(&root, "Makefile", "serve:\n\tpython -m http.server\n");
        write(&root, "justfile", "dev:\n    echo\n");
        write(
            &root,
            "composer.json",
            r#"{"scripts":{"serve":"php -S x","post-install-cmd":"x"}}"#,
        );
        write(&root, "Cargo.toml", "[package]\nname=\"x\"\n");
        write(&root, "src/main.rs", "fn main(){}");
        write(&root, "compose.yml", "services: {}");
        write(
            &root,
            "pyproject.toml",
            "[project.scripts]\nrunit = \"x:main\"\n",
        );
        write(&root, "uv.lock", "");
        assert_eq!(
            ids(&root),
            vec![
                "pkg:dev",
                "pkg:build",
                "make:serve",
                "just:dev",
                "composer:serve",
                "cargo:run",
                "compose:up",
                "py:runit"
            ]
        );
        let fp = detect_stack(&root);
        let specs: std::collections::HashMap<String, (String, Vec<String>)> = discover(&root, &fp)
            .into_iter()
            .map(|s| (s.option.id, (s.program, s.args)))
            .collect();
        let get = |id: &str| specs[id].clone();
        assert_eq!(
            get("pkg:dev"),
            ("bun".into(), vec!["run".into(), "dev".into()])
        );
        assert_eq!(get("make:serve"), ("make".into(), vec!["serve".into()]));
        assert_eq!(get("just:dev"), ("just".into(), vec!["dev".into()]));
        assert_eq!(
            get("composer:serve"),
            ("composer".into(), vec!["run-script".into(), "serve".into()])
        );
        assert_eq!(get("cargo:run"), ("cargo".into(), vec!["run".into()]));
        assert_eq!(
            get("compose:up"),
            ("docker".into(), vec!["compose".into(), "up".into()])
        );
        assert_eq!(
            get("py:runit"),
            ("uv".into(), vec!["run".into(), "runit".into()])
        );
    }

    #[test]
    fn poetry_runner_and_library_crates_are_handled() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(
            root,
            "pyproject.toml",
            "[tool.poetry]\nname=\"x\"\n[tool.poetry.scripts]\napp = \"x:main\"\n",
        );
        write(root, "Cargo.toml", "[package]\nname=\"lib-only\"\n");
        let found = discover(root, &detect_stack(root));
        let py = found.iter().find(|s| s.option.id == "py:app").unwrap();
        assert_eq!(
            (py.program.as_str(), py.args.clone()),
            ("poetry", vec!["run".to_string(), "app".to_string()])
        );
        assert!(
            found.iter().all(|s| s.option.id != "cargo:run"),
            "no binary target"
        );
    }

    #[test]
    fn nothing_is_invented_for_empty_or_broken_projects() {
        let tmp = TempDir::new().unwrap();
        assert!(ids(tmp.path()).is_empty());
        write(tmp.path(), "package.json", "{ broken");
        write(tmp.path(), "composer.json", "nope");
        write(tmp.path(), "pyproject.toml", "= =");
        write(tmp.path(), "Makefile", "\0\u{1}garbage without colon");
        assert!(ids(tmp.path()).is_empty());
    }

    #[test]
    fn oversized_files_and_malicious_names_are_ignored() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(
            root,
            "package.json",
            r#"{"scripts":{"--evil":"x","ok":"y","a b":"z"}}"#,
        );
        write(
            root,
            "Makefile",
            &format!("t:\n{}", " ".repeat(2 * 1024 * 1024)),
        );
        assert_eq!(ids(root), vec!["pkg:ok"]);
    }

    #[test]
    fn resolve_only_accepts_ids_that_exist_now() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(root, "package.json", r#"{"scripts":{"dev":"vite"}}"#);
        write(root, "Makefile", "serve:\n\tx\n");
        let fp = detect_stack(root);
        assert_eq!(resolve(root, &fp, "pkg:dev").unwrap().program, "npm");
        assert_eq!(
            resolve(root, &fp, "dev").unwrap().option.id,
            "pkg:dev",
            "legacy bare name"
        );
        assert_eq!(resolve(root, &fp, "make:serve").unwrap().program, "make");
        for bad in [
            "pkg:nope",
            "make:evil; rm -rf /",
            "bash",
            "-c",
            "make:",
            "just:dev",
        ] {
            assert!(resolve(root, &fp, bad).is_none(), "{bad}");
        }
        let command = to_command(root, &resolve(root, &fp, "make:serve").unwrap());
        assert_eq!(command.script.as_deref(), Some("make:serve"));
        assert_eq!(command.label, "make serve");
        assert_eq!(command.cwd, root.to_string_lossy());
    }

    #[test]
    fn port_support_adds_env_and_manager_correct_flag_only_for_known_servers() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write(
            root,
            "package.json",
            r#"{"scripts":{"dev":"vite"},"devDependencies":{"vite":"1"}}"#,
        );
        let fp = detect_stack(root);
        let base = to_command(root, &resolve(root, &fp, "pkg:dev").unwrap());
        let (npm, env) = with_port(&base, &fp, 4321);
        assert_eq!(npm.args, vec!["run", "dev", "--", "--port", "4321"]);
        assert_eq!(env, vec![("PORT".to_string(), "4321".to_string())]);

        write(root, "bun.lock", "");
        let fp = detect_stack(root);
        let base = to_command(root, &resolve(root, &fp, "pkg:dev").unwrap());
        assert_eq!(
            with_port(&base, &fp, 4321).0.args,
            vec!["run", "dev", "--port", "4321"]
        );

        // Unknown stack: only the environment variable.
        let other = TempDir::new().unwrap();
        write(
            other.path(),
            "package.json",
            r#"{"scripts":{"start":"node x"}}"#,
        );
        let fp = detect_stack(other.path());
        let base = to_command(
            other.path(),
            &resolve(other.path(), &fp, "pkg:start").unwrap(),
        );
        let (same, env) = with_port(&base, &fp, 9000);
        assert_eq!(same.args, base.args);
        assert_eq!(env.len(), 1);
        // Non-package options never get flags.
        let mk = PrimaryCommand {
            script: Some("make:serve".into()),
            ..base
        };
        assert_eq!(with_port(&mk, &fp, 9000).0.args, mk.args);
    }
}
