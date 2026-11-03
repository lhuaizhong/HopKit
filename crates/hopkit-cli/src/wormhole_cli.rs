use clap::{Args, Subcommand};
use hopkit_core::*;
use serde_json::json;
use std::{
    io::{self, IsTerminal, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
#[derive(Args)]
pub struct Options {
    #[arg(long, default_value = "wss://relay.magic-wormhole.io/v1")]
    mailbox_url: String,
    #[arg(long, default_value = "tcp://transit.magic-wormhole.io:4001")]
    transit_relay: String,
    #[arg(long,default_value="auto",value_parser=["auto","direct","relay"])]
    connection: String,
    #[arg(long, default_value_t = 2)]
    code_length: usize,
    #[arg(long, default_value_t = 600)]
    peer_wait_secs: u64,
    #[arg(long)]
    verify: bool,
}
#[derive(Subcommand)]
pub enum Command {
    Send {
        path: String,
        #[command(flatten)]
        options: Options,
        #[arg(long)]
        emit_code: bool,
    },
    Receive {
        code: Option<String>,
        #[arg(long, conflicts_with = "code")]
        code_stdin: bool,
        #[arg(long, default_value = ".")]
        dir: String,
        #[arg(long)]
        output_name: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        max_receive_bytes: Option<String>,
        #[command(flatten)]
        options: Options,
    },
}
fn prompt(text: &str) -> Result<String, ToolError> {
    eprint!("{text}");
    io::stderr().flush()?;
    let mut s = String::new();
    io::stdin().read_line(&mut s)?;
    Ok(s.trim().into())
}
pub fn execute(command: Command, format: &str) -> i32 {
    let result = (|| -> Result<i32, ToolError> {
        let terminal = io::stdin().is_terminal() && io::stderr().is_terminal() && format == "text";
        let (mut config, dir, name, yes, emit_code) = match command {
            Command::Send {
                path,
                options: o,
                emit_code,
            } => (
                config(o, "send", path, String::new()),
                None,
                None,
                false,
                emit_code,
            ),
            Command::Receive {
                code,
                code_stdin,
                dir,
                output_name,
                yes,
                max_receive_bytes,
                options: o,
            } => {
                if !yes && !terminal {
                    return Err(ToolError::new(
                        "INVALID_ARGUMENT",
                        "Noninteractive receive requires --yes",
                    ));
                }
                if !std::path::Path::new(&dir).is_dir() {
                    return Err(ToolError::new("INVALID_ARGUMENT", "--dir must exist"));
                }
                if let Some(n) = &output_name {
                    file_io::safe_name(n)?;
                }
                let code = if code_stdin {
                    let mut s = String::new();
                    io::stdin().read_line(&mut s)?;
                    s.trim().into()
                } else if let Some(code) = code {
                    code
                } else if terminal {
                    prompt("Pairing code / 配对码: ")?
                } else {
                    return Err(ToolError::new(
                        "INVALID_ARGUMENT",
                        "Provide code or --code-stdin",
                    ));
                };
                let mut c = config(o, "receive", String::new(), code);
                c.max_receive_bytes = max_receive_bytes;
                (c, Some(dir), output_name, yes, false)
            }
        };
        if emit_code && format != "jsonl" {
            return Err(ToolError::new(
                "INVALID_ARGUMENT",
                "--emit-code requires send --format jsonl",
            ));
        }
        if config.verify && !terminal {
            return Err(ToolError::new(
                "INVALID_ARGUMENT",
                "--verify requires an interactive text terminal",
            ));
        }
        config.validate()?;
        let cancel = Arc::new(AtomicBool::new(false));
        let handler = cancel.clone();
        ctrlc::set_handler(move || handler.store(true, Ordering::Relaxed))
            .map_err(|_| ToolError::new("INTERNAL_ERROR", "Ctrl+C handler unavailable"))?;
        let control = Arc::new(wormhole::Control::default());
        let ctl = control.clone();
        let worker_cancel = cancel.clone();
        let local_error = Arc::new(std::sync::Mutex::new(None::<ToolError>));
        let worker_error = local_error.clone();
        let dir = dir.map(std::path::PathBuf::from);
        let r = Request {
            tool: "wormhole".into(),
            action: config.operation.clone(),
            target: String::new(),
            wormhole: Some(std::mem::take(&mut config)),
            ..Default::default()
        };
        let worker = std::thread::spawn(move || {
            let mut last_decision = String::new();
            let mut emitted = false;
            while !worker_cancel.load(Ordering::Relaxed) {
                let private = ctl.private_state();
                if !emitted {
                    if let Some(code) = private["code"].as_str() {
                        let out = if emit_code {
                            writeln!(
                                io::stdout(),
                                "{}",
                                json!({"event_type":"wormhole_code","code":code})
                            )
                        } else {
                            writeln!(io::stderr(), "Pairing code / 配对码: {code}")
                        };
                        if out.is_err() {
                            worker_cancel.store(true, Ordering::Relaxed);
                        }
                        emitted = true;
                    }
                }
                let state = ctl.snapshot();
                if !terminal && state["path_error"].is_string() {
                    *worker_error.lock().unwrap() = Some(ToolError::new(
                        state["path_error_code"].as_str().unwrap_or("IO_ERROR"),
                        "Cannot create destination file / 无法创建目标文件",
                    ));
                    worker_cancel.store(true, Ordering::Relaxed);
                    break;
                }
                if let Some(id) = state["decision_id"]
                    .as_str()
                    .filter(|id| *id != last_decision)
                {
                    last_decision = id.into();
                    let decision = if state["phase"] == "waiting_verification" {
                        let answer = prompt(&format!(
                            "Verifier / 校验字符串: {}\nMatch? [y/N] ",
                            private["verifier"].as_str().unwrap_or("")
                        ))
                        .unwrap_or_default();
                        (
                            if answer.eq_ignore_ascii_case("y") {
                                "verify_ok"
                            } else {
                                "reject"
                            },
                            None,
                        )
                    } else {
                        let accepted = yes
                            || prompt(&format!(
                                "Receive {} ({} bytes)? [y/N] ",
                                state["file_name"], state["total_bytes"]
                            ))
                            .unwrap_or_default()
                            .eq_ignore_ascii_case("y");
                        if accepted {
                            let filename = name
                                .as_deref()
                                .or(state["suggested_name"].as_str())
                                .unwrap_or("received-file");
                            let path = dir.as_ref().unwrap().join(filename);
                            if path.exists() {
                                if !terminal {
                                    *worker_error.lock().unwrap() = Some(ToolError::new(
                                        "ALREADY_EXISTS",
                                        "Destination exists; choose another filename / 目标已存在，请另选文件名",
                                    ));
                                    worker_cancel.store(true, Ordering::Relaxed);
                                    break;
                                }
                                eprintln!("Destination exists; enter a new filename");
                                let new = prompt("Filename: ").unwrap_or_default();
                                if file_io::safe_name(&new).is_err() {
                                    worker_cancel.store(true, Ordering::Relaxed);
                                    break;
                                }
                                (
                                    "accept",
                                    Some(
                                        dir.as_ref()
                                            .unwrap()
                                            .join(new)
                                            .to_string_lossy()
                                            .into_owned(),
                                    ),
                                )
                            } else {
                                ("accept", Some(path.to_string_lossy().into_owned()))
                            }
                        } else {
                            ("reject", None)
                        }
                    };
                    let _ = ctl.action(id, decision.0, decision.1);
                    if state["path_error"].is_string() && !terminal {
                        worker_cancel.store(true, Ordering::Relaxed);
                    }
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        });
        let emit = |v: serde_json::Value| {
            if format != "json" && writeln!(io::stdout(), "{v}").is_err() {
                cancel.store(true, Ordering::Relaxed);
            }
        };
        let mut result = wormhole::run(&r, &cancel, &emit, &control);
        if let Some(mut e) = local_error.lock().unwrap().take() {
            e.data = Some(control.snapshot());
            result = Err(e);
        }
        cancel.store(true, Ordering::Relaxed);
        // A terminal prompt may still be waiting for input; process exit closes stdin.
        if worker.is_finished() {
            let _ = worker.join();
        }
        let exit = result
            .as_ref()
            .err()
            .map(|e| super::exit_code(&e.code))
            .unwrap_or(0);
        let id = control.snapshot()["task_id"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let mut value = envelope(&id, "wormhole", result);
        if format == "jsonl" {
            value["event_type"] = json!("finished");
        }
        if writeln!(io::stdout(), "{value}").is_err() {
            return Ok(4);
        }
        Ok(exit)
    })();
    match result {
        Ok(code) => code,
        Err(e) => {
            if format == "text" {
                eprintln!("{e}");
            } else {
                let _ = writeln!(io::stdout(), "{}", envelope("", "wormhole", Err(e.clone())));
            }
            super::exit_code(&e.code)
        }
    }
}
fn config(o: Options, operation: &str, path: String, code: String) -> wormhole::Config {
    wormhole::Config {
        operation: operation.into(),
        send_path: path,
        code,
        mailbox_url: o.mailbox_url,
        transit_relay: o.transit_relay,
        connection_mode: o.connection,
        code_length: o.code_length,
        verify: o.verify,
        peer_wait_secs: o.peer_wait_secs,
        ..Default::default()
    }
}
