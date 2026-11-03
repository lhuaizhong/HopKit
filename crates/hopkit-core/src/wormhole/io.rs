use super::Control;
use crate::{ToolError, file_io};
use futures::io::{AsyncRead, AsyncWrite};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
pub struct Reader {
    pub file: async_fs::File,
    pub hash: Sha256,
    pub control: Arc<Control>,
}
impl AsyncRead for Reader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        let n = futures::ready!(Pin::new(&mut self.file).poll_read(cx, buf))?;
        self.hash.update(&buf[..n]);
        if n == 0 {
            self.control.state.lock().unwrap().public["sha256"] =
                serde_json::json!(format!("{:x}", self.hash.clone().finalize()));
        }
        Poll::Ready(Ok(n))
    }
}
type Closing = Pin<Box<dyn Future<Output = std::io::Result<()>> + Send>>;
pub struct Writer {
    file: Option<async_fs::File>,
    temp: Option<file_io::Temporary>,
    dest: PathBuf,
    expected: u64,
    written: u64,
    hash: Sha256,
    closing: Option<Closing>,
    control: Arc<Control>,
    _guard: Vec<std::fs::File>,
}
impl Writer {
    pub fn create(path: &Path, size: u64, control: Arc<Control>) -> Result<Self, ToolError> {
        file_io::safe_name(path.file_name().and_then(|s| s.to_str()).unwrap_or(""))?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        file_io::no_link(parent)?;
        let guard = file_io::guard_directory(parent)?;
        let dest = parent.canonicalize()?.join(path.file_name().unwrap());
        if dest.symlink_metadata().is_ok() {
            return Err(ToolError::new(
                "ALREADY_EXISTS",
                "Destination exists; choose another path / 目标已存在，请另选路径",
            ));
        }
        if fs2::available_space(parent)? < size.saturating_add(64 * 1024 * 1024) {
            return Err(ToolError::new(
                "STORAGE_FULL",
                "Insufficient free space (64 MiB reserve)",
            ));
        }
        let (file, temp) = file_io::create_temp(&dest)?;
        Ok(Self {
            file: Some(async_fs::File::from(file)),
            temp: Some(temp),
            dest,
            expected: size,
            written: 0,
            hash: Sha256::new(),
            closing: None,
            control,
            _guard: guard,
        })
    }
}
impl AsyncWrite for Writer {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        if self.closing.is_some() || buf.len() as u64 > self.expected - self.written {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "File exceeds declared length",
            )));
        }
        let n = futures::ready!(Pin::new(self.file.as_mut().unwrap()).poll_write(cx, buf))?;
        self.hash.update(&buf[..n]);
        self.written += n as u64;
        Poll::Ready(Ok(n))
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(self.file.as_mut().unwrap()).poll_flush(cx)
    }
    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        if self.closing.is_none() {
            if self.written != self.expected {
                return Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Incomplete file",
                )));
            }
            let mut file = self.file.take().unwrap();
            let temp = self.temp.take().unwrap();
            let dest = self.dest.clone();
            let control = self.control.clone();
            let hash = format!("{:x}", self.hash.clone().finalize());
            control.phase("finalizing");
            control.state.lock().unwrap().committing = true;
            self.closing = Some(Box::pin(async move {
                let result = async {
                    use futures::io::AsyncWriteExt;
                    file.flush().await?;
                    file.sync_all().await?;
                    drop(file);
                    smol::unblock(move || {
                        file_io::publish(temp, &dest)
                            .map(|warning| (dest, warning))
                            .map_err(|e| std::io::Error::other(e.to_string()))
                    })
                    .await
                }
                .await;
                let mut s = control.state.lock().unwrap();
                s.committing = false;
                match result {
                    Ok((dest, warning)) => {
                        if let Some(w) = warning {
                            s.public["cleanup_warning"] = w.into();
                        }
                        s.public["local_saved"] = true.into();
                        s.public["save_path"] = dest.to_string_lossy().to_string().into();
                        s.public["sha256"] = hash.into();
                        Ok(())
                    }
                    Err(e) => {
                        s.public["storage_error"] = e.to_string().into();
                        Err(e)
                    }
                }
            }));
        }
        self.closing.as_mut().unwrap().as_mut().poll(cx)
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        drop(self.file.take());
        if let Some(mut temp) = self.temp.take() {
            let result = temp.cleanup();
            let mut s = self.control.state.lock().unwrap();
            s.public["partial_cleanup"] =
                serde_json::json!(if result.is_ok() { "removed" } else { "failed" });
            if let Err(e) = result {
                s.public["cleanup_warning"] =
                    serde_json::json!(format!("Temporary file {}: {e}", temp.path.display()));
            }
        }
    }
}
