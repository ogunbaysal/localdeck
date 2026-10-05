//! Exercises the real Tauri command surface through the mock runtime: argument
//! names on the wire (camelCase), state injection, structured error payloads
//! and result shapes — exactly what the React `invoke` wrappers rely on.

use localdeck_lib::engine::raw::{RawProcess, SocketScan};
use localdeck_lib::engine::Engine;
use localdeck_lib::models::AccessLevel;
use localdeck_lib::persistence::Database;
use localdeck_lib::register_commands;
use localdeck_lib::services::runtime::{Runtime, ScanResult};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{Manager, WebviewWindow};
use tempfile::TempDir;

struct Harness {
    webview: WebviewWindow<tauri::test::MockRuntime>,
    runtime: Arc<Runtime>,
    project_id: i64,
    _tmp: TempDir,
}

fn harness() -> Harness {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("IPC Project ñ");
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::write(
        root.join("package.json"),
        r#"{"name":"ipc-project","scripts":{"dev":"vite"}}"#,
    )
    .unwrap();
    let root = std::fs::canonicalize(root).unwrap();

    let mut db = Database::open_in_memory().unwrap();
    let record = db.upsert_project(&root.to_string_lossy(), 1_000).unwrap();
    let runtime = Arc::new(Runtime::new(db, Engine::new(None)).unwrap());
    runtime.apply_scan(ScanResult {
        processes: vec![RawProcess {
            pid: 1,
            parent_pid: None,
            name: "launchd".into(),
            executable: Some(PathBuf::from("/sbin/launchd")),
            args: vec![],
            cwd: Some(PathBuf::from("/")),
            cpu_usage: 0.0,
            memory_bytes: 1,
            started_at: Some(1),
            access: AccessLevel::Full,
        }],
        sockets: Ok(SocketScan::default()),
        duration_ms: 1,
    });

    let app = register_commands(mock_builder())
        .manage(runtime.clone())
        .build(mock_context(noop_assets()))
        .unwrap();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    // The app must outlive the webview handle used by the test.
    std::mem::forget(app);
    Harness {
        webview,
        runtime,
        project_id: record.id,
        _tmp: tmp,
    }
}

fn call(h: &Harness, cmd: &str, body: Value) -> Result<Value, Value> {
    let response = get_ipc_response(
        &h.webview,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )?;
    match response {
        InvokeResponseBody::Json(text) => Ok(serde_json::from_str(&text).unwrap()),
        InvokeResponseBody::Raw(_) => panic!("unexpected raw response"),
    }
}

#[test]
fn get_snapshot_serialises_the_camel_case_projection_the_ui_expects() {
    let h = harness();
    let snapshot = call(&h, "get_snapshot", json!({})).unwrap();
    assert_eq!(snapshot["scanner"]["intervalMs"], 2000);
    assert!(snapshot["scanner"]["platform"].is_string());
    let project = &snapshot["projects"][0];
    assert_eq!(project["id"], h.project_id);
    assert_eq!(project["runtimeStatus"], "stopped");
    assert_eq!(project["name"], "Ipc Project");
    assert!(project["components"].as_array().unwrap().is_empty());
    assert!(project["stopPending"] == false);
    let _ = h.webview.app_handle();
}

#[test]
fn mutating_commands_accept_camel_case_arguments_and_show_up_in_the_next_snapshot() {
    let h = harness();
    let id = h.project_id;
    call(
        &h,
        "set_project_favorite",
        json!({ "projectId": id, "favorite": true }),
    )
    .unwrap();
    call(
        &h,
        "set_project_override",
        json!({ "projectId": id, "overrides": {
            "displayName": "Renamed", "primaryUrl": "localhost:4321", "preferredEditor": null,
            "preferredTerminal": null, "autoRestart": true } }),
    )
    .unwrap();
    call(
        &h,
        "set_component_kind",
        json!({ "projectId": id, "componentKey": "vite@.", "kind": "web" }),
    )
    .unwrap();
    call(
        &h,
        "set_primary_command",
        json!({ "projectId": id, "option": "pkg:dev" }),
    )
    .unwrap();
    call(
        &h,
        "ignore_project",
        json!({ "projectId": id, "ignored": true }),
    )
    .unwrap();
    h.runtime.apply_scan(ScanResult {
        processes: vec![],
        sockets: Ok(SocketScan::default()),
        duration_ms: 1,
    });
    let snapshot = call(&h, "get_snapshot", json!({})).unwrap();
    let project = &snapshot["projects"][0];
    assert_eq!(project["name"], "Renamed");
    assert_eq!(project["favorite"], true);
    assert_eq!(project["ignored"], true);
    assert_eq!(project["overrides"]["primaryUrl"], "http://localhost:4321");
    assert_eq!(project["primaryUrl"]["source"], "override");
    assert_eq!(project["overrides"]["autoRestart"], true);
    assert_eq!(project["primaryCommand"]["program"], "npm");
    assert_eq!(project["primaryCommand"]["args"], json!(["run", "dev"]));
}

#[test]
fn scripts_and_editors_have_the_documented_shapes() {
    let h = harness();
    let scripts = call(
        &h,
        "list_project_scripts",
        json!({ "projectId": h.project_id }),
    )
    .unwrap();
    assert_eq!(scripts["packageManager"], "npm");
    assert_eq!(scripts["packageManagerDetected"], false);
    assert_eq!(scripts["options"][0]["id"], "pkg:dev");
    assert_eq!(scripts["options"][0]["label"], "npm run dev");
    let editors = call(&h, "list_editors", json!({})).unwrap();
    assert!(editors
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["id"] == "vscode"));
}

#[test]
fn failures_cross_the_boundary_as_structured_error_payloads() {
    let h = harness();
    let missing = call(
        &h,
        "set_project_favorite",
        json!({ "projectId": 9999, "favorite": true }),
    )
    .unwrap_err();
    assert_eq!(missing["code"], "PROJECT_NOT_FOUND");
    assert_eq!(missing["context"]["projectId"], "9999");

    let invalid = call(
        &h,
        "set_project_override",
        json!({ "projectId": h.project_id, "overrides": {
            "displayName": null, "primaryUrl": "file:///etc/passwd", "preferredEditor": null,
            "preferredTerminal": null, "autoRestart": false } }),
    )
    .unwrap_err();
    assert_eq!(invalid["code"], "INVALID_INPUT");
    assert!(invalid["message"].as_str().unwrap().contains("http"));

    let stale = call(
        &h,
        "terminate_process",
        json!({ "pid": 1, "startedAt": null }),
    )
    .unwrap_err();
    assert_eq!(
        stale["code"], "PROCESS_NOT_FOUND",
        "pids not shown in the snapshot are never signalled"
    );

    let no_command = call(&h, "start_project", json!({ "projectId": h.project_id })).unwrap_err();
    assert_eq!(no_command["code"], "NO_PRIMARY_COMMAND");

    let no_url = call(&h, "open_project_url", json!({ "projectId": h.project_id })).unwrap_err();
    assert_eq!(no_url["code"], "INVALID_INPUT");

    let port = call(&h, "open_port_url", json!({ "portId": "1-none" })).unwrap_err();
    assert_eq!(port["code"], "PORT_NOT_FOUND");
}

#[test]
fn stop_report_and_forget_counts_have_the_documented_shapes() {
    let h = harness();
    let report = call(&h, "stop_project", json!({ "projectId": h.project_id })).unwrap();
    assert_eq!(report, json!({ "signaled": [], "failed": [] }));
    let forced = call(
        &h,
        "force_stop_project",
        json!({ "projectId": h.project_id }),
    )
    .unwrap();
    assert_eq!(forced, json!({ "signaled": [], "failed": [] }));
    let removed = call(&h, "forget_stopped_projects", json!({})).unwrap();
    assert_eq!(removed, 1);
}

#[test]
fn settings_round_trip_through_the_full_object_contract() {
    let h = harness();
    let settings = call(&h, "get_settings", json!({})).unwrap();
    assert_eq!(settings["theme"], "system");
    assert_eq!(settings["launchAtLogin"], false);
    let mut changed = settings.clone();
    changed["theme"] = json!("dark");
    changed["forgetAfterDays"] = json!(30);
    let saved = call(&h, "update_settings", json!({ "settings": changed })).unwrap();
    assert_eq!(saved["theme"], "dark");
    assert_eq!(
        call(&h, "get_settings", json!({})).unwrap()["forgetAfterDays"],
        30
    );

    let mut invalid = saved.clone();
    invalid["forgetAfterDays"] = json!(0);
    let err = call(&h, "update_settings", json!({ "settings": invalid })).unwrap_err();
    assert_eq!(err["code"], "INVALID_INPUT");
}

#[test]
fn unknown_commands_do_not_exist_there_is_no_generic_exec() {
    let h = harness();
    for name in ["exec", "run_shell", "query_sql", "write_any_file"] {
        let err = call(&h, name, json!({ "command": "echo hi" })).unwrap_err();
        assert!(
            err.as_str().is_some_and(|m| m.contains("not found")),
            "{name}: {err}"
        );
    }
}

#[test]
fn launch_profiles_free_ports_terminals_and_activity_cross_the_boundary() {
    let h = harness();
    let id = h.project_id;
    call(
        &h,
        "set_launch_profiles",
        json!({ "projectId": id, "profiles": [
            { "name": "Dev", "options": ["pkg:dev"], "program": "rm" } ] }),
    )
    .unwrap();
    h.runtime.apply_scan(ScanResult {
        processes: vec![],
        sockets: Ok(SocketScan::default()),
        duration_ms: 1,
    });
    let snapshot = call(&h, "get_snapshot", json!({})).unwrap();
    let profile = &snapshot["projects"][0]["launchProfiles"][0];
    assert_eq!(profile["name"], "Dev");
    assert_eq!(profile["commands"][0]["program"], "npm");
    assert_eq!(profile["commands"][0]["script"], "pkg:dev");

    let bad = call(
        &h,
        "set_launch_profiles",
        json!({ "projectId": id, "profiles": [{ "name": "x", "options": ["bash"] }] }),
    )
    .unwrap_err();
    assert_eq!(bad["code"], "INVALID_INPUT");

    let port = call(&h, "suggest_free_port", json!({ "preferred": 3000 })).unwrap();
    assert!(port.as_u64().unwrap() >= 3000);
    let terminals = call(&h, "list_terminals", json!({})).unwrap();
    assert!(!terminals.as_array().unwrap().is_empty());
    assert!(terminals[0]["available"].is_boolean());
    let activity = call(
        &h,
        "list_activity",
        json!({ "projectId": null, "limit": 5 }),
    )
    .unwrap();
    assert!(activity.is_array());
    call(&h, "clear_activity", json!({})).unwrap();

    // Starting with an unknown profile id is a structured error, never a spawn.
    let unknown = call(
        &h,
        "start_project",
        json!({ "projectId": id, "profileId": "nope", "port": null }),
    )
    .unwrap_err();
    assert_eq!(unknown["code"], "INVALID_INPUT");
}

#[test]
fn invalid_shortcuts_and_terminals_are_rejected_and_nothing_is_saved() {
    let h = harness();
    let base = call(&h, "get_settings", json!({})).unwrap();
    for (field, value) in [
        ("globalShortcut", json!("K")),
        ("globalShortcut", json!("Ctrl+Banana")),
        ("preferredTerminal", json!("hyper")),
    ] {
        let mut changed = base.clone();
        changed[field] = value;
        let err = call(&h, "update_settings", json!({ "settings": changed })).unwrap_err();
        assert_eq!(err["code"], "INVALID_INPUT", "{field}");
    }
    assert_eq!(call(&h, "get_settings", json!({})).unwrap(), base);
}
