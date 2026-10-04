//! Logical component classification from executable, arguments, cwd, tree and
//! stack evidence. Pure and deterministic; unknown processes fall back to
//! `Other` rather than an unjustified guess.

use super::raw::RawProcess;
use super::stack_detector::StackFingerprint;
use super::tooling::redact_args;
use crate::models::ComponentKind;
use std::collections::HashSet;
use std::path::Path;

const COMMAND_SUMMARY_LIMIT: usize = 120;

/// What one process says about the component it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub priority: u8,
    pub kind: ComponentKind,
    pub technology: Option<&'static str>,
    /// Serves HTTP by design (dev server / web framework).
    pub http: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classification {
    pub kind: ComponentKind,
    pub technology: Option<String>,
    pub name: String,
    /// Stable key for persisting a user's kind correction.
    pub key: String,
    /// Index (into the evidence slice) of the process that decided the label.
    pub primary_index: usize,
    pub command: Option<String>,
}

fn candidate(
    priority: u8,
    kind: ComponentKind,
    technology: Option<&'static str>,
    http: bool,
) -> Option<Candidate> {
    Some(Candidate {
        priority,
        kind,
        technology,
        http,
    })
}

/// Lower-cased program/argument vocabulary for rule matching.
struct Tokens {
    exe: String,
    /// Executable plus the first few positional arguments, reduced to a
    /// basename with script extensions stripped (`.../.bin/vite` → `vite`).
    words: Vec<String>,
    flags: HashSet<String>,
    args_lower: Vec<String>,
}

pub(crate) fn reduce_word(raw: &str) -> String {
    let lower = raw.to_lowercase();
    let last = lower.rsplit('/').next().unwrap_or(&lower);
    // `next-server (v14.1.0)` → `next-server`
    let first = last.split_whitespace().next().unwrap_or(last);
    for ext in [".js", ".mjs", ".cjs", ".ts", ".mts", ".py", ".rb"] {
        if let Some(stripped) = first.strip_suffix(ext) {
            return stripped.to_string();
        }
    }
    first.to_string()
}

impl Tokens {
    fn of(process: &RawProcess) -> Self {
        let exe = process.exe_name();
        let mut words = vec![exe.clone(), reduce_word(&process.name)];
        let mut flags = HashSet::new();
        let mut positional = 0;
        for arg in process.args.iter().skip(1) {
            if arg.starts_with('-') {
                flags.insert(arg.to_lowercase());
                if let Some((name, _)) = arg.split_once('=') {
                    flags.insert(name.to_lowercase());
                }
            } else if positional < 6 {
                words.push(reduce_word(arg));
                positional += 1;
            }
        }
        words.dedup();
        Self {
            exe,
            words,
            flags,
            args_lower: process.args.iter().map(|a| a.to_lowercase()).collect(),
        }
    }

    fn has(&self, word: &str) -> bool {
        self.words.iter().any(|w| w == word)
    }

    fn has_any(&self, words: &[&str]) -> bool {
        words.iter().any(|w| self.has(w))
    }

    fn flag(&self, name: &str) -> bool {
        self.flags.contains(name)
    }

    fn watch(&self) -> bool {
        self.flag("--watch") || self.flag("-w") || self.flag("--watch=true")
    }

    fn exe_is(&self, names: &[&str]) -> bool {
        names.iter().any(|n| self.exe == *n)
    }

    fn exe_starts(&self, prefix: &str) -> bool {
        self.exe.starts_with(prefix)
    }

    /// Positional word at `index` in `words` (0 = exe).
    fn after(&self, word: &str) -> Option<&str> {
        let pos = self.words.iter().position(|w| w == word)?;
        self.words.get(pos + 1).map(String::as_str)
    }
}

/// Relative cwd of `process` inside `project_root` (for tauri-backend evidence).
fn relative_cwd(process: &RawProcess, project_root: &Path) -> Option<String> {
    let cwd = process.cwd.as_ref()?;
    let rel = cwd.strip_prefix(project_root).ok()?;
    Some(rel.to_string_lossy().to_string())
}

/// Rule table for a single process. First matching rule wins.
pub fn process_candidate(
    process: &RawProcess,
    stack: &StackFingerprint,
    project_root: &Path,
) -> Option<Candidate> {
    use ComponentKind::*;
    let t = Tokens::of(process);
    let build = t.has("build") || t.has("export");

    // Desktop
    if t.has_any(&["tauri", "cargo-tauri"]) || t.exe == "tauri" {
        return candidate(90, Desktop, Some("Tauri"), false);
    }
    if t.exe_is(&["electron"]) || t.has("electron") {
        return candidate(90, Desktop, Some("Electron"), false);
    }

    // Databases, caches, queues
    if t.exe_is(&["postgres", "postmaster"]) {
        return candidate(85, Database, Some("PostgreSQL"), false);
    }
    if t.exe_is(&["mysqld", "mariadbd", "mysqld_safe"]) {
        return candidate(85, Database, Some("MySQL"), false);
    }
    if t.exe_is(&["mongod"]) {
        return candidate(85, Database, Some("MongoDB"), false);
    }
    if t.exe_is(&["clickhouse", "clickhouse-server"]) {
        return candidate(85, Database, Some("ClickHouse"), false);
    }
    if t.exe_is(&["cockroach", "couchdb", "influxd", "surreal", "cassandra"]) {
        return candidate(85, Database, None, false);
    }
    if t.exe_is(&["redis-server", "valkey-server", "keydb-server"]) {
        return candidate(85, Cache, Some("Redis"), false);
    }
    if t.exe_is(&["memcached", "dragonfly"]) {
        return candidate(85, Cache, None, false);
    }
    if t.exe_is(&["rabbitmq-server"])
        || (t.exe == "beam.smp" && t.args_lower.iter().any(|a| a.contains("rabbit")))
    {
        return candidate(85, Queue, Some("RabbitMQ"), false);
    }
    if t.exe_is(&["nats-server", "redpanda", "mosquitto", "kafka"]) {
        return candidate(85, Queue, None, false);
    }

    // Workers
    if t.has("artisan") && t.has_any(&["queue:work", "queue:listen", "horizon", "schedule:work"]) {
        return candidate(85, Worker, Some("Laravel"), false);
    }
    if t.has("celery") {
        return candidate(85, Worker, Some("Celery"), false);
    }
    if t.exe_is(&["sidekiq"]) || t.has("sidekiq") {
        return candidate(85, Worker, Some("Sidekiq"), false);
    }

    // Web / API frameworks
    if t.has_any(&["next", "next-server"]) {
        return if build {
            candidate(50, Bundler, Some("Next.js"), false)
        } else {
            candidate(80, Web, Some("Next.js"), true)
        };
    }
    if t.has_any(&["nuxt", "nuxi", "nuxt-dev"]) {
        return if build {
            candidate(50, Bundler, Some("Nuxt"), false)
        } else {
            candidate(80, Web, Some("Nuxt"), true)
        };
    }
    if t.has("astro") {
        return if build {
            candidate(50, Bundler, Some("Astro"), false)
        } else {
            candidate(80, Web, Some("Astro"), true)
        };
    }
    if t.has_any(&["remix-serve", "remix"]) && !build {
        return candidate(80, Web, Some("Remix"), true);
    }
    if t.has_any(&["svelte-kit", "sveltekit"]) {
        return candidate(80, Web, Some("SvelteKit"), !build);
    }
    if t.has("vite") {
        return if build {
            candidate(50, Bundler, Some("Vite"), false)
        } else {
            candidate(75, Web, Some("Vite"), true)
        };
    }
    if t.has("webpack-dev-server") || (t.has("webpack") && t.has("serve")) {
        return candidate(75, Web, Some("webpack"), true);
    }
    if t.has("webpack") {
        return candidate(50, Bundler, Some("webpack"), false);
    }
    if t.has("parcel") {
        return if build {
            candidate(50, Bundler, Some("Parcel"), false)
        } else {
            candidate(75, Web, Some("Parcel"), true)
        };
    }
    if t.has("ng") && t.has("serve") {
        return candidate(75, Web, Some("Angular"), true);
    }
    if t.has("gatsby") && t.has_any(&["develop", "serve"]) {
        return candidate(75, Web, Some("Gatsby"), true);
    }
    if t.has_any(&["storybook", "sb"]) && t.has_any(&["dev", "start"]) {
        return candidate(75, Web, Some("Storybook"), true);
    }
    if t.has("react-scripts") && t.has("start") {
        return candidate(75, Web, Some("Create React App"), true);
    }
    if t.has("vue-cli-service") && t.has("serve") {
        return candidate(75, Web, Some("Vue CLI"), true);
    }
    if t.has_any(&["hugo", "jekyll"]) && t.has_any(&["server", "serve"]) {
        return candidate(75, Web, None, true);
    }
    if t.has("nest") && t.has_any(&["start", "dev"]) {
        return candidate(80, Api, Some("NestJS"), true);
    }
    if t.exe_starts("php") || t.has("php") {
        if t.has("artisan") {
            if t.has("serve") {
                return candidate(80, Web, Some("Laravel"), true);
            }
            return candidate(35, Runtime, Some("Laravel"), false);
        }
        if t.flag("-s") {
            return candidate(80, Web, Some("PHP built-in server"), true);
        }
        return candidate(10, Runtime, Some("PHP"), false);
    }
    if t.has("fastapi") {
        return candidate(80, Api, Some("FastAPI"), true);
    }
    if t.has_any(&["uvicorn", "gunicorn", "hypercorn", "daphne"]) {
        return if stack.has("fastapi") {
            candidate(80, Api, Some("FastAPI"), true)
        } else if stack.has("django") {
            candidate(80, Web, Some("Django"), true)
        } else if stack.has("flask") {
            candidate(80, Web, Some("Flask"), true)
        } else {
            candidate(60, Other, Some("Python"), true)
        };
    }
    if t.has("flask") && t.has("run") {
        return candidate(80, Web, Some("Flask"), true);
    }
    if t.has("manage") && t.has("runserver") {
        return candidate(80, Web, Some("Django"), true);
    }
    if t.has("http.server") || (t.has("http") && t.has("server") && t.exe_starts("python")) {
        return candidate(70, Web, Some("Python http.server"), true);
    }
    if t.has_any(&["rails", "puma"]) && t.has_any(&["server", "s", "puma"]) {
        return candidate(80, Web, Some("Rails"), true);
    }

    // Watchers
    if t.has("tsc") {
        return if t.watch() {
            candidate(60, Watcher, Some("TypeScript"), false)
        } else {
            candidate(40, Cli, Some("TypeScript"), false)
        };
    }
    if t.has_any(&["nodemon", "watchexec", "chokidar"]) {
        return candidate(60, Watcher, None, false);
    }
    if t.has("cargo-watch") || (t.exe == "cargo" && t.has("watch")) {
        return candidate(60, Watcher, Some("Cargo"), false);
    }
    if t.has_any(&["tailwindcss", "tailwind"]) {
        return if t.watch() {
            candidate(60, Watcher, Some("Tailwind CSS"), false)
        } else {
            candidate(40, Cli, Some("Tailwind CSS"), false)
        };
    }
    if t.has("sass") && t.watch() {
        return candidate(60, Watcher, Some("Sass"), false);
    }
    if t.has("vitest") {
        return if t.has("run") {
            candidate(40, Cli, Some("Vitest"), false)
        } else {
            candidate(60, Watcher, Some("Vitest"), false)
        };
    }
    if t.has("jest") {
        return if t.watch() || t.flag("--watchall") {
            candidate(60, Watcher, Some("Jest"), false)
        } else {
            candidate(40, Cli, Some("Jest"), false)
        };
    }
    if t.has("esbuild") {
        return if t.watch() {
            candidate(60, Watcher, Some("esbuild"), false)
        } else {
            candidate(30, Bundler, Some("esbuild"), false)
        };
    }
    if t.has_any(&["rollup", "tsup", "swc", "turbopack"]) {
        return candidate(30, Bundler, None, false);
    }

    // CLI tools
    if t.has_any(&[
        "eslint",
        "prettier",
        "stylelint",
        "playwright",
        "cypress",
        "biome",
    ]) {
        return candidate(40, Cli, None, false);
    }

    // Task runners
    if t.has("turbo") {
        return candidate(20, Runtime, Some("Turborepo"), false);
    }
    if t.has_any(&["nx", "lerna"]) {
        return candidate(20, Runtime, None, false);
    }

    // Tauri backend binary built from this project's src-tauri/
    if stack.has("tauri") {
        let in_backend = relative_cwd(process, project_root)
            .is_some_and(|rel| rel == "src-tauri" || rel.starts_with("src-tauri/"));
        let from_target = process
            .executable
            .as_ref()
            .is_some_and(|exe| exe.components().any(|c| c.as_os_str() == "target"));
        if in_backend && from_target {
            return candidate(70, Desktop, Some("Tauri"), false);
        }
    }

    // Package-manager wrappers: script name is a (weak) hint.
    if t.exe_is(&["bun", "npm", "pnpm", "yarn", "npx", "bunx"]) {
        let tech = match t.exe.as_str() {
            "bun" | "bunx" => "Bun",
            "npm" | "npx" => "npm",
            "pnpm" => "pnpm",
            _ => "Yarn",
        };
        if let Some(script) = t.after("run") {
            if script.contains("worker") || script.contains("queue") {
                return candidate(35, Worker, Some(tech), false);
            }
            if script.contains("watch") {
                return candidate(35, Watcher, Some(tech), false);
            }
        }
        return candidate(10, Runtime, Some(tech), false);
    }

    // Generic language runtimes
    let runtime = match t.exe.as_str() {
        "node" | "nodejs" => Some("Node.js"),
        "deno" => Some("Deno"),
        "ruby" => Some("Ruby"),
        "java" => Some("Java"),
        "dotnet" => Some(".NET"),
        "cargo" => Some("Cargo"),
        "go" => Some("Go"),
        e if e.starts_with("python") => Some("Python"),
        _ => None,
    };
    if let Some(tech) = runtime {
        return candidate(10, Runtime, Some(tech), false);
    }
    None
}

/// Does `process` look like a known dev/web server by itself?
pub fn serves_http_by_design(
    process: &RawProcess,
    stack: &StackFingerprint,
    project_root: &Path,
) -> bool {
    process_candidate(process, stack, project_root).is_some_and(|c| c.http)
}

/// Candidate-based check for infrastructure that must never be HTTP-probed.
pub fn is_non_http_service(process: &RawProcess, stack: &StackFingerprint, root: &Path) -> bool {
    if matches!(
        process_candidate(process, stack, root).map(|c| c.kind),
        Some(ComponentKind::Database | ComponentKind::Cache | ComponentKind::Queue)
    ) {
        return true;
    }
    const NON_HTTP: &[&str] = &[
        "ssh",
        "sshd",
        "sshd-session",
        "ssh-agent",
        "dockerd",
        "docker-proxy",
        "com.docker.backend",
        "containerd",
        "smbd",
        "nmbd",
        "cupsd",
        "rapportd",
        "sharingd",
    ];
    let exe = process.exe_name();
    NON_HTTP.contains(&exe.as_str())
}

/// Ports a process opened for debugging (`--inspect`, remote debugging).
pub fn debug_ports(process: &RawProcess) -> Vec<u16> {
    let mut ports = Vec::new();
    for arg in &process.args {
        let lower = arg.to_lowercase();
        if let Some(rest) = lower
            .strip_prefix("--inspect-brk")
            .or_else(|| lower.strip_prefix("--inspect-wait"))
            .or_else(|| lower.strip_prefix("--inspect"))
        {
            let port = rest
                .strip_prefix('=')
                .and_then(|v| v.rsplit(':').next())
                .and_then(|p| p.parse::<u16>().ok())
                .unwrap_or(9229);
            ports.push(port);
        } else if let Some(value) = lower
            .strip_prefix("--remote-debugging-port=")
            .or_else(|| lower.strip_prefix("--debug-port="))
        {
            if let Ok(port) = value.parse::<u16>() {
                ports.push(port);
            }
        }
    }
    ports
}

fn kind_label(kind: ComponentKind) -> &'static str {
    match kind {
        ComponentKind::Web => "Web",
        ComponentKind::Api => "API",
        ComponentKind::Worker => "Worker",
        ComponentKind::Database => "Database",
        ComponentKind::Cache => "Cache",
        ComponentKind::Queue => "Queue",
        ComponentKind::Bundler => "Bundler",
        ComponentKind::Watcher => "Watcher",
        ComponentKind::Desktop => "Desktop",
        ComponentKind::Cli => "CLI",
        ComponentKind::Runtime => "Runtime",
        ComponentKind::Other => "Other",
    }
}

/// One-line, redacted, length-bounded command summary.
pub fn command_summary(process: &RawProcess) -> Option<String> {
    if process.args.is_empty() {
        return None;
    }
    let mut parts = redact_args(&process.args);
    // argv[0] is often an absolute path; show the program name only.
    let program = parts[0].rsplit('/').next().unwrap_or(&parts[0]).to_string();
    parts[0] = program.trim_start_matches('-').to_string();
    let mut line = parts.join(" ");
    if line.chars().count() > COMMAND_SUMMARY_LIMIT {
        line = line.chars().take(COMMAND_SUMMARY_LIMIT).collect::<String>() + "…";
    }
    Some(line)
}

/// Classify a component given its processes (root first, DFS order).
pub fn classify(
    processes: &[&RawProcess],
    project_root: &Path,
    stack: &StackFingerprint,
) -> Classification {
    let mut best: Option<(usize, Candidate)> = None;
    for (index, process) in processes.iter().enumerate() {
        if let Some(candidate) = process_candidate(process, stack, project_root) {
            // Strictly greater: ties keep the shallowest process.
            if best
                .as_ref()
                .is_none_or(|(_, b)| candidate.priority > b.priority)
            {
                best = Some((index, candidate));
            }
        }
    }

    let root = processes[0];
    let (primary_index, kind, technology) = match &best {
        Some((index, c)) => (*index, c.kind, c.technology.map(str::to_string)),
        None => (0, ComponentKind::Other, None),
    };
    let primary = processes[primary_index];

    let name = technology.clone().unwrap_or_else(|| match kind {
        ComponentKind::Other => root.exe_name(),
        other => kind_label(other).to_string(),
    });
    let rel = relative_cwd(primary, project_root)
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| ".".to_string());
    let identity = technology
        .as_deref()
        .map(str::to_lowercase)
        .unwrap_or_else(|| root.exe_name());

    Classification {
        kind,
        technology,
        name,
        key: format!("{identity}@{rel}"),
        primary_index,
        command: command_summary(root),
    }
}

#[cfg(test)]
mod tests {
    use super::super::process_tree::test_support::proc;
    use super::*;
    use std::path::PathBuf;

    const ROOT: &str = "/work/app";

    fn stack(ids: &[&str]) -> StackFingerprint {
        StackFingerprint {
            stack: ids
                .iter()
                .map(|id| crate::models::StackTag {
                    id: (*id).to_string(),
                    label: (*id).to_string(),
                })
                .collect(),
            ..Default::default()
        }
    }

    fn classify_tree(procs: &[RawProcess], fp: &StackFingerprint) -> Classification {
        let refs: Vec<&RawProcess> = procs.iter().collect();
        classify(&refs, Path::new(ROOT), fp)
    }

    fn one(p: RawProcess) -> Classification {
        classify_tree(&[p], &stack(&[]))
    }

    #[test]
    fn bun_sh_vite_esbuild_tree_is_one_web_component_labelled_vite() {
        let procs = vec![
            proc(10, Some(1), "bun", &["bun", "run", "dev"], Some(ROOT)),
            proc(11, Some(10), "sh", &["sh", "-c", "vite"], Some(ROOT)),
            proc(
                12,
                Some(11),
                "node",
                &["node", "/work/app/node_modules/.bin/vite"],
                Some(ROOT),
            ),
            proc(
                13,
                Some(12),
                "esbuild",
                &["esbuild", "--service=0.21.0", "--ping"],
                Some(ROOT),
            ),
        ];
        let c = classify_tree(&procs, &stack(&[]));
        assert_eq!(c.kind, ComponentKind::Web);
        assert_eq!(c.technology.as_deref(), Some("Vite"));
        assert_eq!(c.name, "Vite");
        // `sh -c vite` is the shallowest process that already says "vite".
        assert_eq!(c.primary_index, 1);
        assert_eq!(c.command.as_deref(), Some("bun run dev"));
        assert_eq!(c.key, "vite@.");
    }

    #[test]
    fn representative_known_commands() {
        let cases: Vec<(RawProcess, ComponentKind, &str)> = vec![
            (
                proc(
                    1,
                    None,
                    "node",
                    &["node", "node_modules/next/dist/bin/next", "dev"],
                    Some(ROOT),
                ),
                ComponentKind::Web,
                "Next.js",
            ),
            (
                proc(
                    1,
                    None,
                    "next-server (v14.1.0)",
                    &["next-server (v14.1.0)"],
                    Some(ROOT),
                ),
                ComponentKind::Web,
                "Next.js",
            ),
            (
                proc(1, None, "tauri", &["tauri", "dev"], Some(ROOT)),
                ComponentKind::Desktop,
                "Tauri",
            ),
            (
                proc(
                    1,
                    None,
                    "php",
                    &["php", "artisan", "queue:work"],
                    Some(ROOT),
                ),
                ComponentKind::Worker,
                "Laravel",
            ),
            (
                proc(1, None, "php", &["php", "artisan", "serve"], Some(ROOT)),
                ComponentKind::Web,
                "Laravel",
            ),
            (
                proc(
                    1,
                    None,
                    "node",
                    &["node", "node_modules/typescript/bin/tsc", "--watch"],
                    Some(ROOT),
                ),
                ComponentKind::Watcher,
                "TypeScript",
            ),
            (
                proc(1, None, "tsc", &["tsc", "-w"], Some(ROOT)),
                ComponentKind::Watcher,
                "TypeScript",
            ),
            (
                proc(1, None, "tsc", &["tsc", "--noEmit"], Some(ROOT)),
                ComponentKind::Cli,
                "TypeScript",
            ),
            (
                proc(
                    1,
                    None,
                    "postgres",
                    &["postgres", "-D", "/data"],
                    Some(ROOT),
                ),
                ComponentKind::Database,
                "PostgreSQL",
            ),
            (
                proc(
                    1,
                    None,
                    "redis-server",
                    &["redis-server", "*:6379"],
                    Some(ROOT),
                ),
                ComponentKind::Cache,
                "Redis",
            ),
            (
                proc(1, None, "rabbitmq-server", &["rabbitmq-server"], Some(ROOT)),
                ComponentKind::Queue,
                "RabbitMQ",
            ),
            (
                proc(1, None, "esbuild", &["esbuild", "--service=1"], Some(ROOT)),
                ComponentKind::Bundler,
                "esbuild",
            ),
            (
                proc(
                    1,
                    None,
                    "node",
                    &["node", "node_modules/.bin/vite", "build"],
                    Some(ROOT),
                ),
                ComponentKind::Bundler,
                "Vite",
            ),
            (
                proc(
                    1,
                    None,
                    "python3",
                    &["python3", "-m", "http.server", "8000"],
                    Some(ROOT),
                ),
                ComponentKind::Web,
                "Python http.server",
            ),
            (
                proc(1, None, "node", &["node", "server.js"], Some(ROOT)),
                ComponentKind::Runtime,
                "Node.js",
            ),
            (
                proc(1, None, "eslint", &["eslint", "."], Some(ROOT)),
                ComponentKind::Cli,
                "eslint",
            ),
            (
                proc(
                    1,
                    None,
                    "node",
                    &["node", "node_modules/.bin/nodemon", "server.js"],
                    Some(ROOT),
                ),
                ComponentKind::Watcher,
                "node",
            ),
        ];
        for (process, kind, tech) in cases {
            let name = process.name.clone();
            let c = one(process);
            assert_eq!(c.kind, kind, "{name}");
            if !tech.is_empty() && c.technology.is_some() {
                assert_eq!(c.technology.as_deref(), Some(tech), "{name}");
            }
        }
    }

    #[test]
    fn python_app_servers_use_stack_evidence_without_overclaiming() {
        let uvicorn = proc(
            1,
            None,
            "python3",
            &["python3", "-m", "uvicorn", "app:app"],
            Some(ROOT),
        );
        let fastapi = classify_tree(std::slice::from_ref(&uvicorn), &stack(&["fastapi"]));
        assert_eq!(
            (fastapi.kind, fastapi.technology.as_deref()),
            (ComponentKind::Api, Some("FastAPI"))
        );
        let django = classify_tree(std::slice::from_ref(&uvicorn), &stack(&["django"]));
        assert_eq!(django.kind, ComponentKind::Web);
        let unknown = classify_tree(&[uvicorn], &stack(&[]));
        assert_eq!(
            unknown.kind,
            ComponentKind::Other,
            "no evidence → Other, not Web/API"
        );
    }

    #[test]
    fn unknown_executable_falls_back_to_other_with_its_name() {
        let c = one(proc(
            1,
            None,
            "my-custom-daemon",
            &["my-custom-daemon", "--flag"],
            Some(ROOT),
        ));
        assert_eq!(c.kind, ComponentKind::Other);
        assert_eq!(c.technology, None);
        assert_eq!(c.name, "my-custom-daemon");
        assert_eq!(c.key, "my-custom-daemon@.");
    }

    #[test]
    fn missing_metadata_still_classifies_safely() {
        let mut p = proc(1, None, "mystery", &[], None);
        p.executable = None;
        p.access = crate::models::AccessLevel::Unavailable;
        let c = one(p);
        assert_eq!(c.kind, ComponentKind::Other);
        assert_eq!(c.command, None);
    }

    #[test]
    fn specific_descendant_beats_generic_wrapper_and_ties_keep_shallowest() {
        let procs = vec![
            proc(10, None, "npm", &["npm", "run", "dev"], Some(ROOT)),
            proc(11, Some(10), "node", &["node", "a.js"], Some(ROOT)),
        ];
        let c = classify_tree(&procs, &stack(&[]));
        assert_eq!(c.technology.as_deref(), Some("npm"));
        assert_eq!(c.primary_index, 0);
    }

    #[test]
    fn tauri_dev_tree_is_one_desktop_component_not_three() {
        let procs = vec![
            proc(10, None, "bun", &["bun", "run", "tauri", "dev"], Some(ROOT)),
            proc(
                11,
                Some(10),
                "node",
                &["node", "node_modules/.bin/tauri", "dev"],
                Some(ROOT),
            ),
            proc(
                12,
                Some(11),
                "cargo",
                &["cargo", "run"],
                Some("/work/app/src-tauri"),
            ),
            proc(13, Some(11), "bun", &["bun", "run", "dev"], Some(ROOT)),
            proc(
                14,
                Some(13),
                "node",
                &["node", "node_modules/.bin/vite"],
                Some(ROOT),
            ),
        ];
        let c = classify_tree(&procs, &stack(&["tauri", "vite"]));
        assert_eq!(c.kind, ComponentKind::Desktop);
        assert_eq!(c.technology.as_deref(), Some("Tauri"));
    }

    #[test]
    fn built_tauri_binary_in_src_tauri_is_desktop() {
        let mut app = proc(
            20,
            Some(12),
            "localdeck",
            &["target/debug/localdeck"],
            Some("/work/app/src-tauri"),
        );
        app.executable = Some(PathBuf::from("/work/app/src-tauri/target/debug/localdeck"));
        let c = classify_tree(&[app], &stack(&["tauri"]));
        assert_eq!(c.kind, ComponentKind::Desktop);
        // Without Tauri in the stack the same binary is not claimed as Tauri.
        let mut other = proc(21, None, "localdeck", &["x"], Some("/work/app/src-tauri"));
        other.executable = Some(PathBuf::from("/work/app/src-tauri/target/debug/localdeck"));
        assert_eq!(
            classify_tree(&[other], &stack(&[])).kind,
            ComponentKind::Other
        );
    }

    #[test]
    fn script_name_hints_are_weak_evidence() {
        let c = one(proc(
            1,
            None,
            "bun",
            &["bun", "run", "queue-worker"],
            Some(ROOT),
        ));
        assert_eq!(c.kind, ComponentKind::Worker);
        let c = one(proc(1, None, "pnpm", &["pnpm", "run", "dev"], Some(ROOT)));
        assert_eq!(c.kind, ComponentKind::Runtime);
    }

    #[test]
    fn key_distinguishes_monorepo_packages() {
        let a = one(proc(
            1,
            None,
            "node",
            &["node", "node_modules/.bin/vite"],
            Some("/work/app/apps/web"),
        ));
        let b = one(proc(
            2,
            None,
            "node",
            &["node", "node_modules/.bin/vite"],
            Some("/work/app/apps/admin"),
        ));
        assert_eq!(a.key, "vite@apps/web");
        assert_eq!(b.key, "vite@apps/admin");
    }

    #[test]
    fn command_summary_is_redacted_and_bounded() {
        let long = "x".repeat(300);
        let p = proc(
            1,
            None,
            "node",
            &["/usr/local/bin/node", "server.js", "--token=abc", &long],
            Some(ROOT),
        );
        let summary = command_summary(&p).unwrap();
        assert!(summary.starts_with("node server.js --token=•••"));
        assert!(summary.chars().count() <= COMMAND_SUMMARY_LIMIT + 1);
        assert!(!summary.contains("abc"));
    }

    #[test]
    fn http_by_design_and_non_http_services() {
        let fp = stack(&[]);
        let root = Path::new(ROOT);
        let vite = proc(
            1,
            None,
            "node",
            &["node", "node_modules/.bin/vite"],
            Some(ROOT),
        );
        assert!(serves_http_by_design(&vite, &fp, root));
        let node = proc(2, None, "node", &["node", "server.js"], Some(ROOT));
        assert!(!serves_http_by_design(&node, &fp, root));
        let pg = proc(3, None, "postgres", &["postgres"], Some(ROOT));
        assert!(is_non_http_service(&pg, &fp, root));
        let sshd = proc(4, None, "sshd", &["sshd"], None);
        assert!(is_non_http_service(&sshd, &fp, root));
        assert!(!is_non_http_service(&vite, &fp, root));
    }

    #[test]
    fn debug_ports_are_detected() {
        let p = proc(
            1,
            None,
            "node",
            &["node", "--inspect", "app.js"],
            Some(ROOT),
        );
        assert_eq!(debug_ports(&p), vec![9229]);
        let p = proc(
            1,
            None,
            "node",
            &["node", "--inspect-brk=127.0.0.1:9333", "app.js"],
            Some(ROOT),
        );
        assert_eq!(debug_ports(&p), vec![9333]);
        let p = proc(
            1,
            None,
            "chrome",
            &["chrome", "--remote-debugging-port=9222"],
            None,
        );
        assert_eq!(debug_ports(&p), vec![9222]);
        assert!(debug_ports(&proc(1, None, "node", &["node", "app.js"], None)).is_empty());
    }
}
