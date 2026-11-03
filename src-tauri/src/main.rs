#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[cfg(all(feature = "native-test", not(debug_assertions)))]
compile_error!("native-test is forbidden in release builds");
use hopkit_core::*;
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
    port: u16,
    tool: String,
    action: String,
    target: String,
    status: String,
    started_at: String,
    cancel: Cancel,
    seq: u64,
    events: VecDeque<Event>,
    result: Option<Value>,
    request: Value,
    latest_stats: Option<Value>,
    monitor_control: Arc<hopkit_core::monitor::Control>,
    wormhole_control: Arc<hopkit_core::wormhole::Control>,
}
impl Task {
    fn record_event(&mut self, body: Value) {
        if body["event_type"] == "wormhole_state" {
            self.latest_stats = Some(body);
            self.seq += 1;
            return;
        }
        if matches!(
            body["event_type"].as_str(),
            Some("monitor_snapshot" | "monitor_finished")
        ) {
            self.latest_stats = Some(body);
            self.seq += 1;
            return;
        }
        if matches!(
            body["event_type"].as_str(),
            Some("stats" | "stopped" | "test_complete")
        ) {
            self.latest_stats = Some(body.clone());
        }
        self.seq += 1;
        self.events.push_back(Event {
            cursor: self.seq,
            body,
        });
        while self.events.len() > 150 {
            self.events.pop_front();
        }
    }
}

#[derive(Clone, Default)]
struct Tasks(
    Arc<Mutex<BTreeMap<String, Task>>>,
    Arc<AtomicBool>,
    Arc<AtomicBool>,
);
#[derive(Serialize)]
struct Snapshot {
    id: String,
    port: u16,
    tool: String,
    action: String,
    target: String,
    status: String,
    started_at: String,
    seq: u64,
    events: Vec<Event>,
    result: Option<Value>,
    request: Value,
    latest_stats: Option<Value>,
}
fn snap(t: &Task, after: u64) -> Snapshot {
    Snapshot {
        id: t.id.clone(),
        port: t.port,
        tool: t.tool.clone(),
        action: t.action.clone(),
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
        result: if after == u64::MAX {
            None
        } else {
            t.result.clone()
        },
        request: t.request.clone(),
        latest_stats: if after == u64::MAX && t.tool != "wormhole" {
            None
        } else {
            t.latest_stats.clone()
        },
    }
}

#[tauri::command]
fn start_task(request: Request, tasks: tauri::State<'_, Tasks>) -> Result<String, String> {
    if request.tool == "wormhole" {
        let config = request
            .wormhole
            .as_ref()
            .ok_or("Wormhole configuration required")?;
        config.validate().map_err(|e| e.to_string())?;
        if request.action != config.operation {
            return Err("Operation and action must match".into());
        }
    }
    if request.tool == "monitor" {
        request
            .monitor
            .as_ref()
            .ok_or("Monitor configuration required")?
            .validate()
            .map_err(|e| e.to_string())?;
    }
    if request.tool == "check" {
        hopkit_core::target::parse(&request.target).map_err(|e| e.to_string())?;
    }
    if tasks.1.load(Ordering::Relaxed) {
        return Err("Application is closing".into());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let cancel = Arc::new(AtomicBool::new(false));
    let monitor_control = Arc::new(hopkit_core::monitor::Control::default());
    let wormhole_control = Arc::new(hopkit_core::wormhole::Control::default());
    wormhole_control.set_task_id(&id);
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
                port: request.port,
                tool: request.tool.clone(),
                action: request.action.clone(),
                target: if request.tool == "wormhole" {
                    request
                        .wormhole
                        .as_ref()
                        .map(|w| {
                            format!(
                                "{} {}",
                                w.operation,
                                std::path::Path::new(&w.send_path)
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                            )
                        })
                        .unwrap_or_default()
                } else if matches!(request.action.as_str(), "serve" | "listen") {
                    format!("{}:{}", request.listen, request.port)
                } else {
                    hopkit_core::target::redact(&request.target)
                },
                status: "running".into(),
                started_at: chrono_now(),
                cancel: cancel.clone(),
                seq: 0,
                events: VecDeque::new(),
                result: None,
                request: serde_json::to_value(&request).map_err(|e| e.to_string())?,
                latest_stats: None,
                monitor_control: monitor_control.clone(),
                wormhole_control: wormhole_control.clone(),
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
                t.record_event(v);
            }
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if request.tool == "wormhole" {
                hopkit_core::wormhole::run(&request, &cancel, &emit, &wormhole_control)
            } else if request.tool == "monitor" {
                hopkit_core::monitor::run(&request, &cancel, &emit, &monitor_control)
            } else {
                run(&request, &cancel, &emit)
            }
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
async fn network_overview() -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        hopkit_core::network::overview(&Arc::new(AtomicBool::new(false))).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn http_file_addresses() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        hopkit_core::http_files::interface_addresses(&Arc::new(AtomicBool::new(false)))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn parse_target(input: String) -> Result<hopkit_core::target::Target, String> {
    hopkit_core::target::parse(&input).map_err(|e| e.to_string())
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
fn monitor_action(
    id: String,
    action: String,
    text: String,
    tasks: tauri::State<'_, Tasks>,
) -> Result<(), String> {
    let m = tasks.0.lock().map_err(|_| "Task state unavailable")?;
    let t = m.get(&id).ok_or("Task not found")?;
    if t.tool != "monitor" || t.status != "running" {
        return Err("Monitor is not running".into());
    }
    t.monitor_control
        .send(action, text)
        .map_err(|e| e.to_string())
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
async fn wormhole_file_info(path: String) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let file = hopkit_core::wormhole::file_info(&path)?;
        Ok(file)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn wormhole_private_state(id: String, tasks: tauri::State<'_, Tasks>) -> Result<Value, String> {
    let map = tasks.0.lock().map_err(|_| "Task state unavailable")?;
    let t = map.get(&id).ok_or("Task not found")?;
    if t.tool != "wormhole" || t.status != "running" {
        return Ok(json!({}));
    }
    Ok(t.wormhole_control.private_state())
}
#[tauri::command]
fn wormhole_action(
    id: String,
    decision_id: String,
    action: String,
    save_path: Option<String>,
    tasks: tauri::State<'_, Tasks>,
) -> Result<(), String> {
    let (control, cancel) = {
        let map = tasks.0.lock().map_err(|_| "Task state unavailable")?;
        let t = map.get(&id).ok_or("Task not found")?;
        if t.tool != "wormhole" || t.status != "running" {
            return Err("Task is not running".into());
        }
        (t.wormhole_control.clone(), t.cancel.clone())
    };
    cancelled(&cancel).map_err(|e| e.to_string())?;
    control
        .action(&decision_id, &action, save_path)
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn wormhole_open_folder(id: String, tasks: tauri::State<'_, Tasks>) -> Result<(), String> {
    let path = {
        let map = tasks.0.lock().map_err(|_| "Task state unavailable")?;
        let t = map.get(&id).ok_or("Task not found")?;
        let state = t.wormhole_control.snapshot();
        if state["local_saved"] != true {
            return Err("File has not been saved".into());
        }
        std::path::PathBuf::from(state["save_path"].as_str().ok_or("Missing path")?)
            .parent()
            .ok_or("Missing directory")?
            .to_path_buf()
    };
    tauri::async_runtime::spawn_blocking(move || {
        std::process::Command::new(if cfg!(target_os = "macos") {
            "/usr/bin/open"
        } else if cfg!(windows) {
            "explorer.exe"
        } else {
            "xdg-open"
        })
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn choose_path(kind: String, suggested_name: Option<String>) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dialog = rfd::FileDialog::new();
        let dialog = if let Some(n) =
            suggested_name.filter(|n| hopkit_core::file_io::safe_name(n).is_ok())
        {
            dialog.set_file_name(n)
        } else {
            dialog
        };
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
async fn export_result_image(png: Vec<u8>, filename: String) -> Result<bool, String> {
    if png.len() > 16 * 1024 * 1024 || !png.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("Invalid PNG or image exceeds 16 MiB".into());
    }
    let safe_name = if filename.starts_with("hopkit-")
        && filename.ends_with(".png")
        && filename
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        filename
    } else {
        "hopkit-result.png".into()
    };
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(safe_name)
            .add_filter("PNG image", &["png"])
            .save_file()
        {
            std::fs::write(path, png).map_err(|e| e.to_string())?;
            Ok(true)
        } else {
            Ok(false)
        }
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn export_result(content: String) -> Result<bool, String> {
    if content.len() > 16 * 1024 * 1024 {
        return Err("Export too large".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name("hopkit-result.json")
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
async fn open_project() -> Result<(), String> {
    open_browser_url("https://github.com/lhuaizhong/HopKit".into()).await
}
#[tauri::command]
async fn open_file_service(
    id: String,
    url: String,
    tasks: tauri::State<'_, Tasks>,
) -> Result<(), String> {
    {
        let map = tasks.0.lock().map_err(|_| "Task state unavailable")?;
        let task = map.get(&id).ok_or("Task not found")?;
        let port = if task.port == 0 { 80 } else { task.port };
        hopkit_core::http_files::validate_browser_url(&url, port).map_err(|e| e.to_string())?;
        if task.tool != "http" || task.status != "running" {
            return Err("HTTP file service is not running".into());
        }
    }
    open_browser_url(url).await
}
async fn open_browser_url(url: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(target_os = "macos")]
        let mut command = {
            let mut command = std::process::Command::new("/usr/bin/open");
            command.arg(&url);
            command
        };
        #[cfg(target_os = "windows")]
        let mut command = {
            use std::os::windows::process::CommandExt;
            let mut command = std::process::Command::new("rundll32.exe");
            command.args(["url.dll,FileProtocolHandler", &url]);
            command.creation_flags(0x08000000);
            command
        };
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let mut command = {
            let mut command = std::process::Command::new("xdg-open");
            command.arg(&url);
            command
        };
        let status = command.status().map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err("Could not open the default browser".into())
        }
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn browse_syslog_logs() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        let Some(path) = rfd::FileDialog::new()
            .set_title("选择已有日志目录 / Select saved log directory")
            .pick_folder()
        else {
            return Ok(());
        };
        let program = if cfg!(target_os = "macos") {
            "/usr/bin/open"
        } else if cfg!(windows) {
            "explorer.exe"
        } else {
            "xdg-open"
        };
        std::process::Command::new(program)
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn app_info() -> Value {
    let os = if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(windows) {
        "Windows"
    } else {
        std::env::consts::OS
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "Apple Silicon",
        "x86_64" => "x64",
        other => other,
    };
    json!({"version":env!("CARGO_PKG_VERSION"),"platform":format!("{os} {arch}"),"os":os,"architecture":std::env::consts::ARCH,"portable":true,"tools":8})
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
fn shutdown(handle: tauri::AppHandle, state: Tasks) {
    state.1.store(true, Ordering::Relaxed);
    if let Ok(mut m) = state.0.lock() {
        for t in m.values_mut() {
            t.cancel.store(true, Ordering::Relaxed);
        }
    }
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
#[cfg(feature = "native-test")]
#[tauri::command]
fn native_test_report(value: Value, app: tauri::AppHandle) -> Result<(), String> {
    let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../test-output/wormhole/native.json");
    std::fs::write(output, serde_json::to_vec_pretty(&value).unwrap())
        .map_err(|e| e.to_string())?;
    app.exit(0);
    Ok(())
}
fn main() {
    let app = tauri::Builder::default()
        .on_page_load(|webview, payload| {
            #[cfg(feature = "native-test")]
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                if let Ok(config) = std::env::var("HOPKIT_NATIVE_TEST_CONFIG") {
                    if let Ok(value) = std::fs::read_to_string(config).and_then(|s| {
                        serde_json::from_str::<Value>(&s).map_err(std::io::Error::other)
                    }) {
                        let script = format!(
                            "window.__HOPKIT_TEST_CONFIG__={};{}",
                            value,
                            include_str!("../../scripts/wormhole-native-driver.js")
                        );
                        let _ = webview.eval(&script);
                    }
                }
            }
            #[cfg(not(feature = "native-test"))]
            let _ = (webview, payload);
        })
        .manage(Tasks::default())
        .invoke_handler(tauri::generate_handler![
            #[cfg(feature = "native-test")]
            native_test_report,
            start_task,
            network_overview,
            http_file_addresses,
            parse_target,
            list_tasks,
            task_poll,
            cancel_task,
            monitor_action,
            wormhole_action,
            wormhole_file_info,
            wormhole_private_state,
            wormhole_open_folder,
            stop_all,
            choose_path,
            export_result,
            export_result_image,
            open_project,
            open_file_service,
            browse_syslog_logs,
            app_info
        ])
        .build(tauri::generate_context!())
        .expect("Cannot start HopKit; check the platform WebView runtime");
    app.run(|app, event| {
        // macOS normally keeps an app alive after the last window closes.
        // This temporary toolbox must stop its listeners when its window closes.
        if let tauri::RunEvent::WindowEvent {
            event: tauri::WindowEvent::CloseRequested { api, .. },
            ..
        } = &event
        {
            api.prevent_close();
            app.exit(0);
        }
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
                #[cfg(target_os = "macos")]
                {
                    if state.2.swap(true, Ordering::Relaxed) { return; }
                    let handle = app.clone();
                    let window = app.get_webview_window("main");
                    // A parented asynchronous Cocoa sheet keeps the application
                    // event loop responsive while the user decides.
                    tauri::async_runtime::spawn(async move {
                        let Some(window) = window else {
                            state.2.store(false, Ordering::Relaxed);
                            return;
                        };
                        let decision = rfd::AsyncMessageDialog::new()
                            .set_parent(&window)
                            .set_title("退出 HopKit / Quit HopKit")
                            .set_description("退出会停止本窗口的全部测试、文件传输和临时监听。其他终端中的独立 CLI 不受影响。\nQuitting stops this window's diagnostics, transfers and listeners. Independent CLI processes remain running.")
                            .set_level(rfd::MessageLevel::Warning)
                            .set_buttons(rfd::MessageButtons::OkCancelCustom("停止并退出 / Stop and quit".into(), "取消 / Cancel".into()))
                            .show().await;
                        if decision == rfd::MessageDialogResult::Custom("停止并退出 / Stop and quit".into()) {
                            shutdown(handle, state);
                        } else {
                            state.2.store(false, Ordering::Relaxed);
                        }
                    });
                }
                #[cfg(not(target_os = "macos"))]
                shutdown(app.clone(), state);
            }
        }
    });
}

#[cfg(test)]
mod trust_tests {
    use super::*;
    #[test]
    fn snapshots_keep_counters_and_redacted_request_after_queue_eviction() {
        let request = Request {
            tool: "tcping".into(),
            target: "127.0.0.1".into(),
            port: 443,
            ftp_password: "must-never-export".into(),
            ..Request::default()
        };
        let mut task = Task {
            id: "test".into(),
            tool: request.tool.clone(),
            target: request.target.clone(),
            port: 443,
            action: String::new(),
            status: "running".into(),
            started_at: chrono_now(),
            cancel: Arc::new(AtomicBool::new(false)),
            seq: 0,
            events: VecDeque::new(),
            result: None,
            request: serde_json::to_value(&request).unwrap(),
            latest_stats: None,
            monitor_control: Arc::new(hopkit_core::monitor::Control::default()),
            wormhole_control: Arc::new(hopkit_core::wormhole::Control::default()),
        };
        for n in 1..=1200 {
            task.record_event(json!({"event_type":"sample","seq":n}));
            task.record_event(json!({"event_type":"stats","sent":n,"received":n*4/5}));
        }
        // No events after this cursor: counters must still be available.
        let snapshot = serde_json::to_value(snap(&task, task.seq)).unwrap();
        assert!(snapshot["events"].as_array().unwrap().is_empty());
        assert_eq!(snapshot["latest_stats"]["sent"], 1200);
        assert_eq!(snapshot["latest_stats"]["received"], 960);
        assert_eq!(task.events.len(), 150);
        assert_eq!(snap(&task, 0).events.len(), 150);
        assert_eq!(snapshot["request"]["port"], 443);
        assert!(snapshot["request"].get("ftp_password").is_none());
        assert!(!snapshot.to_string().contains("must-never-export"));
    }
}
