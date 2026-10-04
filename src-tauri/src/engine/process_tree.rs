//! Parent/child reconstruction from PID/PPID.
//!
//! Defensive against what real process tables contain: missing parents, PID
//! reuse (a "parent" that started after its child), self-parenting and loops.

use super::raw::RawProcess;
use std::collections::{HashMap, HashSet};

/// Parents whose start time is more than this much *after* the child's are
/// treated as PID-reuse artefacts (ms). Clock granularity is one second.
const START_TOLERANCE_MS: i64 = 2_000;
/// Hard bound on tree walks.
const MAX_DEPTH: usize = 256;

#[derive(Debug, Default)]
pub struct ProcessTree {
    parent: HashMap<u32, u32>,
    children: HashMap<u32, Vec<u32>>,
    pids: HashSet<u32>,
}

impl ProcessTree {
    pub fn build(processes: &[RawProcess]) -> Self {
        let by_pid: HashMap<u32, &RawProcess> = processes.iter().map(|p| (p.pid, p)).collect();
        let mut parent: HashMap<u32, u32> = HashMap::new();
        for process in processes {
            let Some(ppid) = process.parent_pid else {
                continue;
            };
            if ppid == process.pid {
                continue;
            }
            let Some(candidate) = by_pid.get(&ppid) else {
                continue;
            };
            if let (Some(child_start), Some(parent_start)) =
                (process.started_at, candidate.started_at)
            {
                if parent_start > child_start + START_TOLERANCE_MS {
                    continue;
                }
            }
            parent.insert(process.pid, ppid);
        }
        break_cycles(&mut parent);

        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        for (child, parent) in &parent {
            children.entry(*parent).or_default().push(*child);
        }
        for list in children.values_mut() {
            list.sort_unstable();
        }
        Self {
            parent,
            children,
            pids: by_pid.keys().copied().collect(),
        }
    }

    pub fn contains(&self, pid: u32) -> bool {
        self.pids.contains(&pid)
    }

    pub fn parent_of(&self, pid: u32) -> Option<u32> {
        self.parent.get(&pid).copied()
    }

    pub fn children_of(&self, pid: u32) -> &[u32] {
        self.children.get(&pid).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Nearest ancestor first.
    pub fn ancestors(&self, pid: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut cursor = pid;
        while let Some(parent) = self.parent_of(cursor) {
            out.push(parent);
            cursor = parent;
            if out.len() >= MAX_DEPTH {
                break;
            }
        }
        out
    }

    /// Pre-order, children visited in ascending PID order. Excludes `pid`.
    pub fn descendants(&self, pid: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut stack: Vec<(u32, usize)> = self
            .children_of(pid)
            .iter()
            .rev()
            .map(|c| (*c, 1))
            .collect();
        while let Some((current, depth)) = stack.pop() {
            out.push(current);
            if depth >= MAX_DEPTH {
                continue;
            }
            for child in self.children_of(current).iter().rev() {
                stack.push((*child, depth + 1));
            }
        }
        out
    }

    /// Processes with no known parent, ascending PID order.
    pub fn roots(&self) -> Vec<u32> {
        let mut roots: Vec<u32> = self
            .pids
            .iter()
            .copied()
            .filter(|pid| !self.parent.contains_key(pid))
            .collect();
        roots.sort_unstable();
        roots
    }
}

/// Remove one edge per cycle (the edge leaving the smallest PID in the loop).
fn break_cycles(parent: &mut HashMap<u32, u32>) {
    let mut starts: Vec<u32> = parent.keys().copied().collect();
    starts.sort_unstable();
    for start in starts {
        let mut seen: Vec<u32> = vec![start];
        let mut cursor = start;
        while let Some(&next) = parent.get(&cursor) {
            if let Some(pos) = seen.iter().position(|p| *p == next) {
                let cycle = &seen[pos..];
                if let Some(min) = cycle.iter().min().copied() {
                    parent.remove(&min);
                }
                break;
            }
            seen.push(next);
            cursor = next;
            if seen.len() > MAX_DEPTH {
                break;
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::super::raw::RawProcess;
    use crate::models::AccessLevel;
    use std::path::PathBuf;

    /// Compact process builder for engine tests.
    pub fn proc(
        pid: u32,
        ppid: Option<u32>,
        name: &str,
        args: &[&str],
        cwd: Option<&str>,
    ) -> RawProcess {
        RawProcess {
            pid,
            parent_pid: ppid,
            name: name.to_string(),
            executable: Some(PathBuf::from(format!("/usr/bin/{name}"))),
            args: args.iter().map(|a| a.to_string()).collect(),
            cwd: cwd.map(PathBuf::from),
            cpu_usage: 0.0,
            memory_bytes: 1_000_000,
            started_at: Some(1_700_000_000_000 + pid as i64),
            access: AccessLevel::Full,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::proc;
    use super::*;

    #[test]
    fn builds_the_bun_sh_vite_esbuild_chain() {
        let procs = vec![
            proc(10, Some(1), "bun", &["bun", "run", "dev"], None),
            proc(11, Some(10), "sh", &["sh", "-c", "vite"], None),
            proc(12, Some(11), "node", &["node", "vite"], None),
            proc(13, Some(12), "esbuild", &["esbuild", "--service"], None),
            proc(1, None, "launchd", &[], None),
        ];
        let tree = ProcessTree::build(&procs);
        assert_eq!(tree.descendants(10), vec![11, 12, 13]);
        assert_eq!(tree.ancestors(13), vec![12, 11, 10, 1]);
        assert_eq!(tree.parent_of(10), Some(1));
        assert_eq!(tree.roots(), vec![1]);
    }

    #[test]
    fn sibling_trees_stay_separate() {
        let procs = vec![
            proc(1, None, "launchd", &[], None),
            proc(10, Some(1), "bun", &[], None),
            proc(11, Some(10), "vite", &[], None),
            proc(20, Some(1), "node", &[], None),
            proc(21, Some(20), "next", &[], None),
        ];
        let tree = ProcessTree::build(&procs);
        assert_eq!(tree.descendants(10), vec![11]);
        assert_eq!(tree.descendants(20), vec![21]);
        assert!(!tree.descendants(10).contains(&21));
    }

    #[test]
    fn descendants_are_preorder_with_ascending_children() {
        let procs = vec![
            proc(1, None, "root", &[], None),
            proc(5, Some(1), "b", &[], None),
            proc(2, Some(1), "a", &[], None),
            proc(3, Some(2), "a1", &[], None),
            proc(6, Some(5), "b1", &[], None),
        ];
        let tree = ProcessTree::build(&procs);
        assert_eq!(tree.descendants(1), vec![2, 3, 5, 6]);
    }

    #[test]
    fn missing_parent_makes_a_root() {
        let procs = vec![proc(50, Some(49), "orphan", &[], None)];
        let tree = ProcessTree::build(&procs);
        assert_eq!(tree.parent_of(50), None);
        assert_eq!(tree.roots(), vec![50]);
    }

    #[test]
    fn self_parent_and_cycles_do_not_loop_forever() {
        let procs = vec![
            proc(7, Some(7), "self", &[], None),
            proc(8, Some(9), "a", &[], None),
            proc(9, Some(8), "b", &[], None),
        ];
        let tree = ProcessTree::build(&procs);
        assert_eq!(tree.parent_of(7), None);
        // The edge leaving the smallest pid of the cycle is dropped.
        assert_eq!(tree.parent_of(8), None);
        assert_eq!(tree.parent_of(9), Some(8));
        assert_eq!(tree.descendants(8), vec![9]);
        assert!(tree.ancestors(9).len() < 5);
    }

    #[test]
    fn parent_started_after_child_is_treated_as_pid_reuse() {
        let mut child = proc(30, Some(20), "child", &[], None);
        child.started_at = Some(1_000_000);
        let mut reused_parent = proc(20, Some(1), "new-occupant", &[], None);
        reused_parent.started_at = Some(9_000_000);
        let tree = ProcessTree::build(&[child, reused_parent]);
        assert_eq!(tree.parent_of(30), None);
    }

    #[test]
    fn unknown_start_times_do_not_break_links() {
        let mut child = proc(30, Some(20), "child", &[], None);
        child.started_at = None;
        let parent = proc(20, None, "parent", &[], None);
        let tree = ProcessTree::build(&[child, parent]);
        assert_eq!(tree.parent_of(30), Some(20));
    }
}
