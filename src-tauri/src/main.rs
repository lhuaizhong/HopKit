#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use netbox_core::*;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::Manager;

#[derive(Clone, Serialize)]
struct Event {
    cursor: u64,
    #[serde(flatten)]
    body: Value,
}
struct Task {
    id: String,
    tool: String,
    target: String,
    status: String,
    started_at: String,
    cancel: Cancel,
    seq: u64,
    events: VecDeque<Event>,
    result: Option<Value>,
}
#[derive(Clone, Default)]
struct Tasks(Arc<Mutex<BTreeMap<String, Task>>>, Arc<AtomicBool>);
#[derive(Serialize)]
struct Snapshot {
    id: String,
    tool: String,
    target: String,
    status: String,
    started_at: String,
    seq: u64,
    events: Vec<Event>,
    result: Option<Value>,
}
fn snap(t: &Task, after: u64) -> Snapshot {
    Snapshot {
        id: t.id.clone(),
        tool: t.tool.clone(),
        target: t.target.clone(),
        status: t.status.clone(),
        started_at: t.started_at.clone(),
        seq: t.seq,
        events: t
            .events
            .iter()
            .filter(|e| e.cursor > after)
            .cloned()
            .collect(),
        result: t.result.clone(),
    }
}

#[tauri::command]
fn start_task(request: Request, tasks: tauri::State<'_, Tasks>) -> Result<String, String> {
    if tasks.1.load(Ordering::Relaxed) {
        return Err("Application is closing".into());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut map = tasks.0.lock().map_err(|_| "Task state unavailable")?;
        if map
            .values()
            .filter(|t| t.status == "running" || t.status == "cancelling")
            .count()
            >= 4
        {
            return Err("At most 4 concurrent tasks / 最多同时运行 4 个任务".into());
        }
        if map.len() >= 40 {
            let oldest = map
                .values()
                .filter(|t| !matches!(t.status.as_str(), "running" | "cancelling"))
                .min_by_key(|t| &t.started_at)
                .map(|t| t.id.clone());
            if let Some(x) = oldest {
                map.remove(&x);
            }
        }
        map.insert(
            id.clone(),
            Task {
                id: id.clone(),
                tool: request.tool.clone(),
                target: if matches!(request.action.as_str(), "serve" | "listen") {
                    format!("{}:{}", request.listen, request.port)
                } else {
                    request.target.clone()
                },
                status: "running".into(),
                started_at: chrono_now(),
                cancel: cancel.clone(),
                seq: 0,
                events: VecDeque::new(),
                result: None,
            },
        );
    }
    let state = tasks.inner().clone();
    let task_id = id.clone();
    std::thread::spawn(move || {
        let emit = |mut v: Value| {
            if v["event_type"] == "log" {
                v["record"]["raw_hex"] = Value::Null;
                if let Some(s) = v["record"]["text"].as_str() {
                    let short = s.chars().take(2000).collect::<String>();
                    v["record"]["text"] = json!(short);
                }
            }
            if let Ok(mut map) = state.0.lock()
                && let Some(t) = map.get_mut(&task_id)
            {
                t.seq += 1;
                t.events.push_back(Event {
                    cursor: t.seq,
                    body: v,
                });
                while t.events.len() > 150 {
                    t.events.pop_front();
                }
            }
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run(&request, &cancel, &emit)
        }))
        .unwrap_or_else(|_| {
            Err(ToolError::new(
                "INTERNAL_ERROR",
                "Worker stopped unexpectedly",
            ))
        });
        let value = envelope(&task_id, &request.tool, result);
        if let Ok(mut map) = state.0.lock()
            && let Some(t) = map.get_mut(&task_id)
        {
            t.status = value["status"].as_str().unwrap_or("failed").into();
            t.result = Some(value);
        }
    });
    Ok(id)
}
#[tauri::command]
fn list_tasks(tasks: tauri::State<'_, Tasks>) -> Vec<Snapshot> {
    tasks
        .0
        .lock()
        .map(|m| {
            m.values()
                .map(|t| {
                    let mut s = snap(t, u64::MAX);
                    s.result = None;
                    s
                })
                .collect()
        })
        .unwrap_or_default()
}
#[tauri::command]
fn task_poll(id: String, after: u64, tasks: tauri::State<'_, Tasks>) -> Result<Snapshot, String> {
    let m = tasks.0.lock().map_err(|_| "Task state unavailable")?;
    m.get(&id)
        .map(|t| snap(t, after))
        .ok_or("Task not found".into())
}
#[tauri::command]
fn cancel_task(id: String, tasks: tauri::State<'_, Tasks>) -> Result<(), String> {
    let mut m = tasks.0.lock().map_err(|_| "Task state unavailable")?;
    let t = m.get_mut(&id).ok_or("Task not found")?;
    if t.status == "running" {
        t.status = "cancelling".into();
        t.cancel.store(true, Ordering::Relaxed);
    }
    Ok(())
}
#[tauri::command]
fn stop_all(tasks: tauri::State<'_, Tasks>) {
    if let Ok(mut m) = tasks.0.lock() {
        for t in m.values_mut() {
            if t.status == "running" {
                t.status = "cancelling".into();
                t.cancel.store(true, Ordering::Relaxed);
            }
        }
    }
}
#[tauri::command]
async fn choose_path(kind: String) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dialog = rfd::FileDialog::new();
        match kind.as_str() {
            "directory" => dialog.pick_folder(),
            "save" => dialog.save_file(),
            _ => dialog.pick_file(),
        }
        .map(|p| p.to_string_lossy().into_owned())
    })
    .await
    .unwrap_or(None)
}
#[tauri::command]
async fn export_result(content: String) -> Result<bool, String> {
    if content.len() > 16 * 1024 * 1024 {
        return Err("Export too large".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name("netbox-result.json")
            .add_filter("JSON", &["json"])
            .save_file()
        {
            std::fs::write(path, content).map_err(|e| e.to_string())?;
            Ok(true)
        } else {
            Ok(false)
        }
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn app_info() -> Value {
    json!({"version":env!("CARGO_PKG_VERSION"),"platform":"Windows x64","portable":true,"tools":7})
}
fn chrono_now() -> String {
    format!(
        "{:020}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    )
}
fn main() {
    let app = tauri::Builder::default()
        .manage(Tasks::default())
        .invoke_handler(tauri::generate_handler![
            start_task,
            list_tasks,
            task_poll,
            cancel_task,
            stop_all,
            choose_path,
            export_result,
            app_info
        ])
        .build(tauri::generate_context!())
        .expect("Cannot start netbox; WebView2 runtime is required");
    app.run(|app, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            let state = app.state::<Tasks>().inner().clone();
            if state.1.load(Ordering::Relaxed) {
                return;
            }
            let active = state
                .0
                .lock()
                .map(|m| {
                    m.values()
                        .any(|t| matches!(t.status.as_str(), "running" | "cancelling"))
                })
                .unwrap_or(false);
            if active {
                api.prevent_exit();
                state.1.store(true, Ordering::Relaxed);
                if let Ok(mut m) = state.0.lock() {
                    for t in m.values_mut() {
                        t.cancel.store(true, Ordering::Relaxed);
                    }
                }
                let handle = app.clone();
                std::thread::spawn(move || {
                    let deadline = Instant::now() + Duration::from_secs(5);
                    loop {
                        let running = state
                            .0
                            .lock()
                            .map(|m| {
                                m.values()
                                    .any(|t| matches!(t.status.as_str(), "running" | "cancelling"))
                            })
                            .unwrap_or(false);
                        if !running || Instant::now() > deadline {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    handle.exit(0);
                });
            }
        }
    });
}
