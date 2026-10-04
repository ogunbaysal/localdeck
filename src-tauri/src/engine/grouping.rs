//! Attribute processes to projects and form logical component trees.
//!
//! Rules (docs/engine/01-discovery-and-classification.md):
//! * a process is attributed by its cwd's project root; a child whose cwd is
//!   unknown or outside any project inherits its parent's project (process-tree
//!   evidence) — except below a session host, whose own cwd says nothing about
//!   where its children were started;
//! * interactive shells / terminal hosts are *transparent*: never components,
//!   their children become roots; editor/language-server tooling is dropped
//!   together with its whole subtree; idle leaf shells are ignored;
//! * a component is the highest attributed process whose parent is not part of
//!   the same project, plus its project-attributed descendants. Sibling trees
//!   are never merged because they share a cwd.

use super::process_tree::ProcessTree;
use super::project_detector::ProjectRoot;
use super::raw::RawProcess;
use super::tooling::{is_editor_tooling, is_session_host, is_shell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentGroup {
    pub root_pid: u32,
    /// Root first, then descendants in tree order.
    pub pids: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectGroup {
    pub root: ProjectRoot,
    /// Ascending by root pid.
    pub components: Vec<ComponentGroup>,
}

/// Where a pid ended up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PidOwner {
    pub root: PathBuf,
    /// Index into `ProjectGroup::components` of that project.
    pub component: usize,
}

#[derive(Debug, Default)]
pub struct Grouping {
    /// Ascending by root path.
    pub projects: Vec<ProjectGroup>,
    pub owner_of: HashMap<u32, PidOwner>,
}

pub fn group_processes(
    processes: &[RawProcess],
    tree: &ProcessTree,
    detect: &mut dyn FnMut(&Path) -> Option<ProjectRoot>,
) -> Grouping {
    let by_pid: HashMap<u32, &RawProcess> = processes.iter().map(|p| (p.pid, p)).collect();

    // Editor tooling is dropped together with its entire subtree.
    let mut dropped: HashSet<u32> = HashSet::new();
    for process in processes {
        if is_editor_tooling(process) && !dropped.contains(&process.pid) {
            dropped.insert(process.pid);
            dropped.extend(tree.descendants(process.pid));
        }
    }

    // Session hosts are transparent; idle leaf shells are ignored.
    let transparent = |process: &RawProcess| -> bool {
        is_session_host(process) || (is_shell(process) && tree.children_of(process.pid).is_empty())
    };

    // Attribution, parents before children.
    let mut attribution: HashMap<u32, ProjectRoot> = HashMap::new();
    let mut order: Vec<u32> = Vec::new();
    for root_pid in tree.roots() {
        order.push(root_pid);
        order.extend(tree.descendants(root_pid));
    }
    for pid in order {
        if dropped.contains(&pid) {
            continue;
        }
        let Some(process) = by_pid.get(&pid) else {
            continue;
        };
        let own = process.cwd.as_deref().and_then(&mut *detect);
        let resolved = own.or_else(|| {
            let parent = tree.parent_of(pid)?;
            let parent_process = by_pid.get(&parent)?;
            if is_session_host(parent_process) {
                return None;
            }
            attribution.get(&parent).cloned()
        });
        if let Some(root) = resolved {
            attribution.insert(pid, root);
        }
    }

    let is_member = |pid: u32, root: &ProjectRoot| -> bool {
        if dropped.contains(&pid) {
            return false;
        }
        let Some(process) = by_pid.get(&pid) else {
            return false;
        };
        attribution.get(&pid).is_some_and(|r| r.path == root.path) && !transparent(process)
    };

    // Component roots per project.
    let mut roots_by_project: BTreeMap<PathBuf, (ProjectRoot, Vec<u32>)> = BTreeMap::new();
    let mut pids: Vec<u32> = attribution.keys().copied().collect();
    pids.sort_unstable();
    for pid in pids {
        let root = &attribution[&pid];
        if !is_member(pid, root) {
            continue;
        }
        let parent_is_member = tree
            .parent_of(pid)
            .is_some_and(|parent| is_member(parent, root));
        if parent_is_member {
            continue;
        }
        roots_by_project
            .entry(root.path.clone())
            .or_insert_with(|| (root.clone(), Vec::new()))
            .1
            .push(pid);
    }

    let mut grouping = Grouping::default();
    for (path, (root, root_pids)) in roots_by_project {
        let mut components = Vec::new();
        for root_pid in root_pids {
            let mut member_pids = vec![root_pid];
            // Descend only through members of this project.
            let mut stack: Vec<u32> = tree.children_of(root_pid).iter().rev().copied().collect();
            while let Some(pid) = stack.pop() {
                if !is_member(pid, &root) {
                    continue;
                }
                member_pids.push(pid);
                stack.extend(tree.children_of(pid).iter().rev().copied());
            }
            let index = components.len();
            for pid in &member_pids {
                grouping.owner_of.insert(
                    *pid,
                    PidOwner {
                        root: path.clone(),
                        component: index,
                    },
                );
            }
            components.push(ComponentGroup {
                root_pid,
                pids: member_pids,
            });
        }
        grouping.projects.push(ProjectGroup { root, components });
    }
    grouping
}

#[cfg(test)]
mod tests {
    use super::super::process_tree::test_support::proc;
    use super::*;
    use crate::models::RootKind;

    /// Fake detector: the longest configured root that prefixes the cwd.
    fn detector(roots: &'static [&'static str]) -> impl FnMut(&Path) -> Option<ProjectRoot> {
        move |cwd: &Path| {
            roots
                .iter()
                .filter(|r| cwd.starts_with(r))
                .max_by_key(|r| r.len())
                .map(|r| ProjectRoot {
                    path: PathBuf::from(r),
                    kind: RootKind::Git,
                })
        }
    }

    fn run(procs: &[RawProcess], roots: &'static [&'static str]) -> Grouping {
        let tree = ProcessTree::build(procs);
        group_processes(procs, &tree, &mut detector(roots))
    }

    const APP: &str = "/work/app";

    #[test]
    fn bun_sh_vite_esbuild_is_one_component_not_four() {
        let procs = vec![
            proc(1, None, "launchd", &[], Some("/")),
            proc(10, Some(1), "bun", &["bun", "run", "dev"], Some(APP)),
            proc(11, Some(10), "sh", &["sh", "-c", "vite"], Some(APP)),
            proc(12, Some(11), "node", &["node", "vite"], Some(APP)),
            proc(13, Some(12), "esbuild", &["esbuild"], Some(APP)),
        ];
        let g = run(&procs, &["/work/app"]);
        assert_eq!(g.projects.len(), 1);
        let components = &g.projects[0].components;
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].root_pid, 10);
        assert_eq!(components[0].pids, vec![10, 11, 12, 13]);
    }

    #[test]
    fn sibling_trees_with_the_same_cwd_stay_separate_components() {
        let procs = vec![
            proc(1, None, "launchd", &[], Some("/")),
            proc(10, Some(1), "bun", &["bun", "run", "dev"], Some(APP)),
            proc(11, Some(10), "node", &["node", "vite"], Some(APP)),
            proc(20, Some(1), "bun", &["bun", "run", "worker"], Some(APP)),
            proc(21, Some(20), "node", &["node", "worker.js"], Some(APP)),
        ];
        let g = run(&procs, &["/work/app"]);
        let components = &g.projects[0].components;
        assert_eq!(components.len(), 2);
        assert_eq!(components[0].pids, vec![10, 11]);
        assert_eq!(components[1].pids, vec![20, 21]);
    }

    #[test]
    fn tauri_frontend_and_src_tauri_processes_share_one_project() {
        let procs = vec![
            proc(1, None, "launchd", &[], Some("/")),
            proc(10, Some(1), "bun", &["bun", "run", "dev"], Some(APP)),
            proc(11, Some(10), "node", &["node", "vite"], Some(APP)),
            proc(
                20,
                Some(1),
                "cargo",
                &["cargo", "run"],
                Some("/work/app/src-tauri"),
            ),
        ];
        let g = run(&procs, &["/work/app"]);
        assert_eq!(g.projects.len(), 1);
        assert_eq!(g.projects[0].components.len(), 2);
    }

    #[test]
    fn nested_git_root_is_a_separate_project() {
        let procs = vec![
            proc(1, None, "launchd", &[], Some("/")),
            proc(10, Some(1), "node", &["node", "a.js"], Some(APP)),
            proc(
                20,
                Some(1),
                "node",
                &["node", "b.js"],
                Some("/work/app/vendor/lib/src"),
            ),
        ];
        let g = run(&procs, &["/work/app", "/work/app/vendor/lib"]);
        assert_eq!(g.projects.len(), 2);
    }

    #[test]
    fn interactive_session_shell_is_transparent_and_children_become_roots() {
        let procs = vec![
            proc(1, None, "launchd", &[], Some("/")),
            proc(5, Some(1), "zsh", &["-zsh"], Some(APP)),
            proc(10, Some(5), "bun", &["bun", "run", "dev"], Some(APP)),
            proc(11, Some(10), "node", &["node", "vite"], Some(APP)),
        ];
        let g = run(&procs, &["/work/app"]);
        let comps = &g.projects[0].components;
        assert_eq!(comps.len(), 1);
        assert_eq!(
            comps[0].root_pid, 10,
            "the user's shell must not be a stop target"
        );
        assert!(!g.owner_of.contains_key(&5));
    }

    #[test]
    fn idle_shell_alone_does_not_make_a_project_run() {
        let procs = vec![
            proc(1, None, "launchd", &[], Some("/")),
            proc(5, Some(1), "zsh", &["-zsh"], Some(APP)),
            proc(6, Some(1), "sh", &["sh", "-c", "sleep 1"], Some(APP)),
        ];
        let g = run(&procs, &["/work/app"]);
        assert!(g.projects.is_empty());
    }

    #[test]
    fn editor_tooling_subtree_is_dropped() {
        let procs = vec![
            proc(1, None, "launchd", &[], Some("/")),
            proc(10, Some(1), "rust-analyzer", &["rust-analyzer"], Some(APP)),
            proc(11, Some(10), "cargo", &["cargo", "check"], Some(APP)),
            proc(12, Some(11), "rustc", &["rustc"], Some(APP)),
            proc(
                20,
                Some(1),
                "git",
                &["git", "fsmonitor--daemon", "run"],
                Some(APP),
            ),
        ];
        assert!(run(&procs, &["/work/app"]).projects.is_empty());
    }

    #[test]
    fn unknown_cwd_child_inherits_but_not_below_a_session_host() {
        let mut hidden = proc(12, Some(11), "esbuild", &["esbuild"], None);
        hidden.cwd = None;
        let mut under_shell = proc(31, Some(30), "node", &["node", "x.js"], None);
        under_shell.cwd = None;
        let procs = vec![
            proc(1, None, "launchd", &[], Some("/")),
            proc(10, Some(1), "bun", &["bun", "run", "dev"], Some(APP)),
            proc(
                11,
                Some(10),
                "node",
                &["node", "vite"],
                Some("/tmp/elsewhere"),
            ),
            hidden,
            proc(30, Some(1), "zsh", &["-zsh"], Some(APP)),
            under_shell,
        ];
        let g = run(&procs, &["/work/app"]);
        assert_eq!(g.projects.len(), 1);
        // child with a cwd outside any project and the cwd-less grandchild both inherit
        assert_eq!(g.projects[0].components[0].pids, vec![10, 11, 12]);
        assert!(
            !g.owner_of.contains_key(&31),
            "no inheritance through a session host"
        );
    }

    #[test]
    fn child_in_another_project_is_not_pulled_into_its_parent() {
        let procs = vec![
            proc(1, None, "launchd", &[], Some("/")),
            proc(10, Some(1), "turbo", &["turbo", "run", "dev"], Some(APP)),
            proc(
                11,
                Some(10),
                "node",
                &["node", "server.js"],
                Some("/work/other"),
            ),
        ];
        let g = run(&procs, &["/work/app", "/work/other"]);
        assert_eq!(g.projects.len(), 2);
        let app = g
            .projects
            .iter()
            .find(|p| p.root.path == Path::new(APP))
            .unwrap();
        assert_eq!(app.components[0].pids, vec![10]);
        let other = g.owner_of.get(&11).unwrap();
        assert_eq!(other.root, PathBuf::from("/work/other"));
    }

    #[test]
    fn global_services_without_cwd_evidence_stay_unattributed() {
        let procs = vec![
            proc(1, None, "launchd", &[], Some("/")),
            proc(
                40,
                Some(1),
                "postgres",
                &["postgres"],
                Some("/opt/homebrew/var/postgresql@16"),
            ),
            proc(
                41,
                Some(1),
                "redis-server",
                &["redis-server"],
                Some("/opt/homebrew/var"),
            ),
            proc(42, Some(1), "mystery", &[], None),
        ];
        let g = run(&procs, &["/work/app"]);
        assert!(g.projects.is_empty());
        assert!(g.owner_of.is_empty());
    }

    #[test]
    fn infra_started_from_the_project_directory_is_attributed_by_cwd_evidence() {
        let procs = vec![
            proc(1, None, "launchd", &[], Some("/")),
            proc(40, Some(1), "redis-server", &["redis-server"], Some(APP)),
        ];
        let g = run(&procs, &["/work/app"]);
        assert_eq!(g.projects[0].components[0].pids, vec![40]);
    }

    #[test]
    fn owner_map_points_pids_to_component_indexes() {
        let procs = vec![
            proc(1, None, "launchd", &[], Some("/")),
            proc(10, Some(1), "bun", &["bun", "run", "dev"], Some(APP)),
            proc(11, Some(10), "node", &["node", "vite"], Some(APP)),
            proc(20, Some(1), "node", &["node", "api.js"], Some(APP)),
        ];
        let g = run(&procs, &["/work/app"]);
        assert_eq!(g.owner_of[&11].component, 0);
        assert_eq!(g.owner_of[&20].component, 1);
    }

    #[test]
    fn deep_process_chains_terminate() {
        let mut procs = vec![proc(1, None, "launchd", &[], Some("/"))];
        for i in 2..400u32 {
            procs.push(proc(i, Some(i - 1), "node", &["node", "x"], Some(APP)));
        }
        let g = run(&procs, &["/work/app"]);
        assert_eq!(g.projects.len(), 1);
    }
}
