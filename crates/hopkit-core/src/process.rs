//! Bounded subprocess adapter: no shell, no detached children on cancellation.
use crate::runtime::*;
use std::{
    io::Read,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
pub struct Output {
    pub code: i32,
    pub stdout: String,
}
pub fn capture(
    cmd: &mut Command,
    c: &Cancel,
    timeout: Duration,
    limit: usize,
) -> Result<Output, ToolError> {
    cancelled(c)?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            ToolError::new(
                "UNSUPPORTED_CAPABILITY",
                format!("System adapter unavailable / 系统适配器不可用: {e}"),
            )
        })?;
    let pipe = child.stdout.take().unwrap();
    let overflow = Arc::new(AtomicBool::new(false));
    let flag = overflow.clone();
    let reader = std::thread::spawn(move || {
        let mut data = Vec::new();
        let mut pipe = pipe.take((limit + 1) as u64);
        let _ = pipe.read_to_end(&mut data);
        if data.len() > limit {
            flag.store(true, Ordering::Relaxed);
        }
        data
    });
    let start = Instant::now();
    let result = loop {
        if c.load(Ordering::Relaxed) {
            break Err(ToolError::new(
                "CANCELLED",
                "Operation cancelled / 操作已取消",
            ));
        }
        if start.elapsed() > timeout {
            break Err(ToolError::new(
                "TIMEOUT",
                "System adapter timed out / 系统适配器超时",
            ));
        }
        if overflow.load(Ordering::Relaxed) {
            break Err(ToolError::new(
                "RESOURCE_EXHAUSTED",
                "Adapter output limit reached / 输出已达上限",
            ));
        }
        match child.try_wait() {
            Ok(Some(s)) => break Ok(s.code().unwrap_or(-1)),
            Err(e) => break Err(e.into()),
            _ => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    if result.is_err() {
        let _ = child.kill();
    }
    let _ = child.wait();
    let bytes = reader.join().unwrap_or_default();
    let code = result?;
    if bytes.len() > limit {
        return Err(ToolError::new(
            "RESOURCE_EXHAUSTED",
            "Adapter output limit reached",
        ));
    }
    Ok(Output {
        code,
        stdout: String::from_utf8_lossy(&bytes).into_owned(),
    })
}
pub fn text(program: &str, args: &[&str], c: &Cancel) -> Result<String, ToolError> {
    let output = capture(
        Command::new(program).args(args),
        c,
        Duration::from_secs(6),
        512 * 1024,
    )?;
    if output.code != 0 {
        return Err(ToolError::new(
            "SYSTEM_QUERY_FAILED",
            "System query failed / 系统查询失败",
        ));
    }
    Ok(output.stdout)
}
