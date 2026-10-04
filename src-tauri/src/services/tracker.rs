//! Snapshot-to-snapshot comparison that derives *meaningful* activity events
//! (project started/stopped, port opened/closed, ambiguous ownership, a
//! previously-listening component disappearing). Metrics never produce events.
//!
//! Stop/close/lost transitions need two consecutive absent scans so a single
//! flaky scan cannot spam the log.

use crate::models::{ActivityKind, ActivityMetadata, ProjectId};
use crate::persistence::NewActivity;
use std::collections::{BTreeMap, BTreeSet, HashMap};

const ABSENT_SCANS_BEFORE_TRANSITION: u8 = 2;

/// What one scan saw for one project.
#[derive(Debug, Clone, Default)]
pub struct Observation {
    pub id: ProjectId,
    pub active: bool,
    pub ports: BTreeSet<u16>,
    /// `(component key, name, has a listening port)`.
    pub components: Vec<(String, String, bool)>,
    pub conflicts: BTreeSet<u16>,
}

#[derive(Debug, Default)]
struct Anchor {
    name: String,
    absent: u8,
    lost: bool,
}

#[derive(Debug, Default)]
struct Memory {
    confirmed_active: bool,
    absent: u8,
    ports: BTreeMap<u16, u8>,
    anchors: BTreeMap<String, Anchor>,
    conflicts: BTreeSet<u16>,
}

#[derive(Debug)]
pub struct Tracker {
    projects: HashMap<ProjectId, Memory>,
    first_scan: bool,
}

impl Default for Tracker {
    fn default() -> Self {
        Self {
            projects: HashMap::new(),
            first_scan: true,
        }
    }
}

fn event(
    project: ProjectId,
    kind: ActivityKind,
    metadata: ActivityMetadata,
    now_ms: i64,
) -> NewActivity {
    NewActivity {
        project_id: Some(project),
        kind,
        metadata,
        created_at: now_ms,
    }
}

impl Tracker {
    pub fn observe(&mut self, observations: &[Observation], now_ms: i64) -> Vec<NewActivity> {
        let mut events = Vec::new();
        let first_scan = self.first_scan;
        for obs in observations {
            let memory = self.projects.entry(obs.id).or_default();
            if obs.active {
                memory.absent = 0;
                if !memory.confirmed_active {
                    memory.confirmed_active = true;
                    memory.ports.clear();
                    memory.anchors.clear();
                    memory.conflicts.clear();
                    events.push(event(
                        obs.id,
                        ActivityKind::ProjectStarted,
                        ActivityMetadata {
                            detected: first_scan,
                            ..Default::default()
                        },
                        now_ms,
                    ));
                    // Ports already open when we first look were not "opened" by us.
                    if first_scan {
                        for port in &obs.ports {
                            memory.ports.insert(*port, 0);
                        }
                    }
                }
                Self::track_ports(obs, memory, now_ms, &mut events);
                Self::track_components(obs, memory, now_ms, &mut events);
                Self::track_conflicts(obs, memory, now_ms, &mut events);
            } else if memory.confirmed_active {
                memory.absent += 1;
                if memory.absent >= ABSENT_SCANS_BEFORE_TRANSITION {
                    memory.confirmed_active = false;
                    memory.absent = 0;
                    memory.ports.clear();
                    memory.anchors.clear();
                    memory.conflicts.clear();
                    events.push(event(
                        obs.id,
                        ActivityKind::ProjectStopped,
                        ActivityMetadata::default(),
                        now_ms,
                    ));
                }
            }
        }
        // Forget projects that disappeared from the registry entirely.
        let live: BTreeSet<ProjectId> = observations.iter().map(|o| o.id).collect();
        self.projects.retain(|id, _| live.contains(id));
        self.first_scan = false;
        events
    }

    fn track_ports(
        obs: &Observation,
        memory: &mut Memory,
        now_ms: i64,
        events: &mut Vec<NewActivity>,
    ) {
        for port in &obs.ports {
            if memory.ports.insert(*port, 0).is_none() {
                events.push(event(
                    obs.id,
                    ActivityKind::PortOpened,
                    ActivityMetadata {
                        port: Some(*port),
                        ..Default::default()
                    },
                    now_ms,
                ));
            }
        }
        let mut closed = Vec::new();
        for (port, absent) in memory.ports.iter_mut() {
            if !obs.ports.contains(port) {
                *absent += 1;
                if *absent >= ABSENT_SCANS_BEFORE_TRANSITION {
                    closed.push(*port);
                }
            }
        }
        for port in closed {
            memory.ports.remove(&port);
            events.push(event(
                obs.id,
                ActivityKind::PortClosed,
                ActivityMetadata {
                    port: Some(port),
                    ..Default::default()
                },
                now_ms,
            ));
        }
    }

    fn track_components(
        obs: &Observation,
        memory: &mut Memory,
        now_ms: i64,
        events: &mut Vec<NewActivity>,
    ) {
        for (key, name, has_port) in &obs.components {
            if *has_port {
                let anchor = memory.anchors.entry(key.clone()).or_default();
                anchor.name = name.clone();
                anchor.absent = 0;
                anchor.lost = false;
            }
        }
        let present: BTreeSet<&String> = obs.components.iter().map(|(k, _, _)| k).collect();
        for (key, anchor) in memory.anchors.iter_mut() {
            if present.contains(key) {
                anchor.absent = 0;
                anchor.lost = false;
                continue;
            }
            anchor.absent = anchor.absent.saturating_add(1);
            if anchor.absent >= ABSENT_SCANS_BEFORE_TRANSITION && !anchor.lost {
                anchor.lost = true;
                events.push(event(
                    obs.id,
                    ActivityKind::RuntimeLost,
                    ActivityMetadata {
                        component: Some(anchor.name.clone()),
                        ..Default::default()
                    },
                    now_ms,
                ));
            }
        }
    }

    fn track_conflicts(
        obs: &Observation,
        memory: &mut Memory,
        now_ms: i64,
        events: &mut Vec<NewActivity>,
    ) {
        for port in &obs.conflicts {
            if memory.conflicts.insert(*port) {
                events.push(event(
                    obs.id,
                    ActivityKind::PortConflict,
                    ActivityMetadata {
                        port: Some(*port),
                        ..Default::default()
                    },
                    now_ms,
                ));
            }
        }
        memory.conflicts.retain(|p| obs.conflicts.contains(p));
    }

    /// Components that had a listening port and then vanished while the
    /// project stayed active: `(key, name)`.
    pub fn lost_components(&self, id: ProjectId) -> Vec<(String, String)> {
        self.projects
            .get(&id)
            .map(|m| {
                m.anchors
                    .iter()
                    .filter(|(_, a)| a.lost)
                    .map(|(k, a)| (k.clone(), a.name.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Whether the project has been continuously active (post-hysteresis).
    pub fn is_confirmed_active(&self, id: ProjectId) -> bool {
        self.projects.get(&id).is_some_and(|m| m.confirmed_active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(id: i64, active: bool, ports: &[u16], comps: &[(&str, bool)]) -> Observation {
        Observation {
            id,
            active,
            ports: ports.iter().copied().collect(),
            components: comps
                .iter()
                .map(|(k, p)| (k.to_string(), k.to_uppercase(), *p))
                .collect(),
            conflicts: BTreeSet::new(),
        }
    }

    fn kinds(events: &[NewActivity]) -> Vec<ActivityKind> {
        events.iter().map(|e| e.kind).collect()
    }

    #[test]
    fn projects_already_running_at_launch_are_marked_detected_without_port_spam() {
        let mut tracker = Tracker::default();
        let events = tracker.observe(&[obs(1, true, &[1420], &[("vite", true)])], 100);
        assert_eq!(kinds(&events), vec![ActivityKind::ProjectStarted]);
        assert!(events[0].metadata.detected);
        // Same state next scan: nothing new.
        assert!(tracker
            .observe(&[obs(1, true, &[1420], &[("vite", true)])], 200)
            .is_empty());
    }

    #[test]
    fn a_project_that_starts_later_is_a_real_start_with_port_opened() {
        let mut tracker = Tracker::default();
        tracker.observe(&[obs(1, false, &[], &[])], 100);
        let events = tracker.observe(&[obs(1, true, &[3000], &[("next", true)])], 200);
        assert_eq!(
            kinds(&events),
            vec![ActivityKind::ProjectStarted, ActivityKind::PortOpened]
        );
        assert!(!events[0].metadata.detected);
        assert_eq!(events[1].metadata.port, Some(3000));
    }

    #[test]
    fn stop_needs_two_absent_scans_and_one_flaky_scan_is_ignored() {
        let mut tracker = Tracker::default();
        tracker.observe(&[obs(1, true, &[3000], &[("a", true)])], 100);
        assert!(tracker.observe(&[obs(1, false, &[], &[])], 200).is_empty());
        // Flaps back: no stop, no second start.
        assert!(tracker
            .observe(&[obs(1, true, &[3000], &[("a", true)])], 300)
            .is_empty());
        assert!(tracker.observe(&[obs(1, false, &[], &[])], 400).is_empty());
        let events = tracker.observe(&[obs(1, false, &[], &[])], 500);
        assert_eq!(kinds(&events), vec![ActivityKind::ProjectStopped]);
        assert!(!tracker.is_confirmed_active(1));
        // Staying stopped emits nothing further.
        assert!(tracker.observe(&[obs(1, false, &[], &[])], 600).is_empty());
    }

    #[test]
    fn port_open_and_close_are_debounced() {
        let mut tracker = Tracker::default();
        tracker.observe(&[obs(1, true, &[], &[("a", false)])], 100);
        let opened = tracker.observe(&[obs(1, true, &[3000], &[("a", true)])], 200);
        assert_eq!(kinds(&opened), vec![ActivityKind::PortOpened]);
        assert!(tracker
            .observe(&[obs(1, true, &[], &[("a", true)])], 300)
            .is_empty());
        let closed = tracker.observe(&[obs(1, true, &[], &[("a", true)])], 400);
        assert_eq!(kinds(&closed), vec![ActivityKind::PortClosed]);
        assert_eq!(closed[0].metadata.port, Some(3000));
    }

    #[test]
    fn a_vanished_listening_component_becomes_lost_only_while_the_project_stays_up() {
        let mut tracker = Tracker::default();
        tracker.observe(
            &[obs(
                1,
                true,
                &[5173, 4000],
                &[("vite", true), ("api", true)],
            )],
            100,
        );
        assert!(tracker
            .observe(&[obs(1, true, &[4000], &[("api", true)])], 200)
            .iter()
            .all(|e| e.kind != ActivityKind::RuntimeLost));
        let events = tracker.observe(&[obs(1, true, &[4000], &[("api", true)])], 300);
        assert!(events.iter().any(|e| e.kind == ActivityKind::RuntimeLost
            && e.metadata.component.as_deref() == Some("VITE")));
        assert_eq!(
            tracker.lost_components(1),
            vec![("vite".to_string(), "VITE".to_string())]
        );
        // It comes back → no longer lost.
        tracker.observe(
            &[obs(
                1,
                true,
                &[5173, 4000],
                &[("vite", true), ("api", true)],
            )],
            400,
        );
        assert!(tracker.lost_components(1).is_empty());
    }

    #[test]
    fn a_full_stop_is_not_reported_as_lost_components() {
        let mut tracker = Tracker::default();
        tracker.observe(&[obs(1, true, &[5173], &[("vite", true)])], 100);
        tracker.observe(&[obs(1, false, &[], &[])], 200);
        let events = tracker.observe(&[obs(1, false, &[], &[])], 300);
        assert_eq!(kinds(&events), vec![ActivityKind::ProjectStopped]);
        assert!(tracker.lost_components(1).is_empty());
    }

    #[test]
    fn components_without_ports_are_never_anchors() {
        let mut tracker = Tracker::default();
        tracker.observe(
            &[obs(1, true, &[], &[("worker", false), ("api", false)])],
            100,
        );
        tracker.observe(&[obs(1, true, &[], &[("api", false)])], 200);
        let events = tracker.observe(&[obs(1, true, &[], &[("api", false)])], 300);
        assert!(events.is_empty());
    }

    #[test]
    fn conflicts_are_reported_once_until_they_clear() {
        let mut tracker = Tracker::default();
        let mut with_conflict = obs(1, true, &[3000], &[("a", true)]);
        with_conflict.conflicts = [3000u16].into_iter().collect();
        let first = tracker.observe(&[with_conflict.clone()], 100);
        assert!(first.iter().any(|e| e.kind == ActivityKind::PortConflict));
        let again = tracker.observe(&[with_conflict.clone()], 200);
        assert!(again.is_empty());
        tracker.observe(&[obs(1, true, &[3000], &[("a", true)])], 300);
        let reappears = tracker.observe(&[with_conflict], 400);
        assert!(reappears
            .iter()
            .any(|e| e.kind == ActivityKind::PortConflict));
    }

    #[test]
    fn metric_changes_never_generate_events() {
        let mut tracker = Tracker::default();
        tracker.observe(&[obs(1, true, &[3000], &[("a", true)])], 100);
        for t in 1..50 {
            assert!(tracker
                .observe(&[obs(1, true, &[3000], &[("a", true)])], 100 + t)
                .is_empty());
        }
    }

    #[test]
    fn removed_projects_are_forgotten() {
        let mut tracker = Tracker::default();
        tracker.observe(&[obs(1, true, &[1], &[("a", true)])], 100);
        tracker.observe(&[], 200);
        assert!(!tracker.is_confirmed_active(1));
    }
}
