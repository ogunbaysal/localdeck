//! Deterministic lists of processes that are *not* project runtime:
//! interactive shells / terminal hosts (transparent session hosts) and editor
//! or language-server tooling (dropped with their whole subtree).
//!
//! These are heuristics, kept in one reviewable place. Everything else with
//! cwd/process-tree evidence is a project process.

use super::raw::RawProcess;

const SHELLS: &[&str] = &[
    "sh", "bash", "zsh", "fish", "dash", "ksh", "csh", "tcsh", "nu", "elvish", "xonsh", "pwsh",
];

/// Terminal emulators, multiplexers and remote-login hosts.
const SESSION_HOSTS: &[&str] = &[
    "tmux",
    "tmux: server",
    "tmux: client",
    "screen",
    "zellij",
    "login",
    "sshd",
    "sshd-session",
    "ssh",
    "mosh-server",
    "su",
    "script",
    "terminal",
    "iterm2",
    "alacritty",
    "kitty",
    "wezterm",
    "wezterm-gui",
    "wezterm-mux-server",
    "ghostty",
    "gnome-terminal-server",
    "konsole",
    "xfce4-terminal",
    "tilix",
    "foot",
    "terminator",
];

const EDITOR_TOOLING: &[&str] = &[
    "rust-analyzer",
    "rust-analyzer-proc-macro-srv",
    "gopls",
    "pyright",
    "pyright-langserver",
    "pylsp",
    "lua-language-server",
    "intelephense",
    "phpactor",
    "eslint_d",
    "prettierd",
    "tailwindcss-language-server",
    "typescript-language-server",
    "tsserver",
    "clangd",
    "jdtls",
    "taplo",
    "marksman",
    "ruff-lsp",
    "copilot-language-server",
    "nvim",
    "vim",
    "vi",
    "emacs",
    "nano",
    "hx",
    "helix",
    "micro",
    "code",
    "code-insiders",
    "cursor",
    "zed",
    "zed-editor",
    "idea",
    "webstorm",
    "phpstorm",
    "pycharm",
    "rustrover",
    "goland",
    "fsnotifier",
    "watchman",
    "git",
    "gitstatusd",
    "ssh-agent",
    "gpg-agent",
    "claude",
    "codex",
    "aider",
    "gemini",
];

const EDITOR_TOOLING_PREFIXES: &[&str] = &["code helper", "cursor helper", "visual studio code"];

const EDITOR_ARG_MARKERS: &[&str] = &[
    "/tsserver",
    "tsserver.js",
    "typingsinstaller",
    "language-server",
    "languageserver",
    "/.vscode/extensions/",
    "/.cursor/extensions/",
    "/.vscode-server/",
    "lsp-proxy",
];

fn name_matches(process: &RawProcess, list: &[&str]) -> bool {
    let exe = process.exe_name();
    let name = process.name_lower();
    list.iter().any(|entry| *entry == exe || *entry == name)
}

pub fn is_shell(process: &RawProcess) -> bool {
    let exe = process.exe_name();
    SHELLS.contains(&exe.as_str()) || SHELLS.contains(&process.name_lower().as_str())
}

/// A shell started without `-c` and without a script operand is a user session.
/// Shells with unknown arguments are treated as interactive.
pub fn is_interactive_shell(process: &RawProcess) -> bool {
    if !is_shell(process) {
        return false;
    }
    for arg in process.args.iter().skip(1) {
        if arg == "-" || arg == "--" {
            continue;
        }
        if arg.starts_with("--") {
            if arg == "--command" {
                return false;
            }
            continue;
        }
        if let Some(cluster) = arg.strip_prefix('-') {
            if cluster.contains('c') {
                return false;
            }
            continue;
        }
        return false;
    }
    true
}

/// Interactive shells, terminal emulators, multiplexers, remote-login hosts.
pub fn is_session_host(process: &RawProcess) -> bool {
    is_interactive_shell(process) || name_matches(process, SESSION_HOSTS)
}

/// Editors, language servers and similar tooling: never project runtime.
pub fn is_editor_tooling(process: &RawProcess) -> bool {
    if name_matches(process, EDITOR_TOOLING) {
        return true;
    }
    let name = process.name_lower();
    if EDITOR_TOOLING_PREFIXES.iter().any(|p| name.starts_with(p)) {
        return true;
    }
    process.args.iter().any(|arg| {
        let lower = arg.to_lowercase();
        EDITOR_ARG_MARKERS.iter().any(|m| lower.contains(m))
    })
}

const SENSITIVE_KEYWORDS: &[&str] = &[
    "token",
    "secret",
    "password",
    "passwd",
    "apikey",
    "api-key",
    "api_key",
    "auth",
    "credential",
    "private-key",
    "private_key",
];

fn is_sensitive_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    SENSITIVE_KEYWORDS.iter().any(|k| lower.contains(k))
}

fn mask_url_credentials(arg: &str) -> String {
    let Some(scheme_end) = arg.find("://") else {
        return arg.to_string();
    };
    let rest = &arg[scheme_end + 3..];
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let Some(at) = authority.rfind('@') else {
        return arg.to_string();
    };
    let userinfo = &authority[..at];
    let Some(colon) = userinfo.find(':') else {
        return arg.to_string();
    };
    format!(
        "{}://{}:•••{}{}",
        &arg[..scheme_end],
        &userinfo[..colon],
        &authority[at..],
        &rest[authority_end..]
    )
}

/// Masks obvious secrets (`--token=…`, `--password x`, `user:pass@host` URLs)
/// before arguments leave the engine. Best effort, never exhaustive.
pub fn redact_args(args: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut mask_next = false;
    for arg in args {
        if mask_next {
            mask_next = false;
            out.push("•••".to_string());
            continue;
        }
        let stripped = arg.trim_start_matches('-');
        if let Some((name, _value)) = stripped.split_once('=') {
            if is_sensitive_name(name) {
                let prefix = &arg[..arg.len() - stripped.len()];
                out.push(format!("{prefix}{name}=•••"));
                continue;
            }
        } else if arg.starts_with('-') && is_sensitive_name(stripped) {
            mask_next = true;
            out.push(arg.clone());
            continue;
        }
        out.push(mask_url_credentials(arg));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::process_tree::test_support::proc;
    use super::*;

    #[test]
    fn interactive_shells_are_session_hosts_but_command_shells_are_not() {
        assert!(is_session_host(&proc(1, None, "zsh", &["-zsh"], None)));
        assert!(is_session_host(&proc(
            1,
            None,
            "bash",
            &["bash", "--login"],
            None
        )));
        assert!(is_session_host(&proc(1, None, "fish", &[], None)));
        assert!(is_session_host(&proc(
            1,
            None,
            "zsh",
            &["zsh", "-il"],
            None
        )));
        assert!(!is_session_host(&proc(
            1,
            None,
            "sh",
            &["sh", "-c", "vite"],
            None
        )));
        assert!(!is_session_host(&proc(
            1,
            None,
            "bash",
            &["bash", "-lc", "bun dev"],
            None
        )));
        assert!(!is_session_host(&proc(
            1,
            None,
            "bash",
            &["bash", "dev.sh"],
            None
        )));
        assert!(!is_session_host(&proc(
            1,
            None,
            "node",
            &["node", "server.js"],
            None
        )));
    }

    #[test]
    fn terminals_and_multiplexers_are_session_hosts() {
        for name in ["tmux", "screen", "login", "sshd", "ghostty", "Alacritty"] {
            assert!(
                is_session_host(&proc(1, None, name, &[name], None)),
                "{name}"
            );
        }
        assert!(is_session_host(&proc(1, None, "tmux: server", &[], None)));
    }

    #[test]
    fn editor_and_language_tooling_is_recognised() {
        for name in [
            "rust-analyzer",
            "gopls",
            "nvim",
            "git",
            "fsnotifier",
            "Code Helper (Plugin)",
        ] {
            assert!(
                is_editor_tooling(&proc(1, None, name, &[name], None)),
                "{name}"
            );
        }
        assert!(is_editor_tooling(&proc(
            1,
            None,
            "node",
            &[
                "node",
                "/x/node_modules/typescript/lib/tsserver.js",
                "--useInferredProjectPerProjectRoot"
            ],
            None
        )));
        assert!(is_editor_tooling(&proc(
            1,
            None,
            "node",
            &["node", "/Users/me/.vscode/extensions/foo/server.js"],
            None
        )));
        assert!(!is_editor_tooling(&proc(
            1,
            None,
            "node",
            &["node", "vite"],
            None
        )));
        assert!(!is_editor_tooling(&proc(
            1,
            None,
            "bun",
            &["bun", "run", "dev"],
            None
        )));
    }

    #[test]
    fn redacts_secret_flags_values_and_url_passwords() {
        let args: Vec<String> = [
            "node",
            "server.js",
            "--token=abc123",
            "--api-key",
            "sk-live-999",
            "--port=3000",
            "postgres://admin:hunter2@localhost:5432/app",
            "https://example.com/path",
            "--password",
            "p",
            "PASSWORD=zzz",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let redacted = redact_args(&args);
        assert_eq!(
            redacted,
            vec![
                "node",
                "server.js",
                "--token=•••",
                "--api-key",
                "•••",
                "--port=3000",
                "postgres://admin:•••@localhost:5432/app",
                "https://example.com/path",
                "--password",
                "•••",
                "PASSWORD=•••",
            ]
        );
    }
}
