use crate::runtime::*;
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};
pub fn safe_name(n: &str) -> Result<(), ToolError> {
    let base = n.split('.').next().unwrap_or("").to_uppercase();
    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if n.is_empty()
        || n.len() > 240
        || n.starts_with('.')
        || n.ends_with(['.', ' '])
        || n.chars()
            .any(|c| c.is_control() || "/\\:*?\"<>|".contains(c))
        || reserved.contains(&base.as_str())
    {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "Only a safe filename in the root directory is supported / 仅支持根目录内的安全文件名",
        ));
    }
    Ok(())
}
pub(crate) fn no_link(p: &Path) -> Result<(), ToolError> {
    let m = fs::symlink_metadata(p)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x400 != 0 {
            return Err(ToolError::new(
                "PERMISSION_DENIED",
                "Reparse points are not allowed",
            ));
        }
    }
    if m.file_type().is_symlink() {
        return Err(ToolError::new(
            "PERMISSION_DENIED",
            "Symlinks are not allowed",
        ));
    }
    Ok(())
}

// Hold every canonical ancestor open without FILE_SHARE_DELETE. Remote requests
// cannot race a parent-directory rename/junction replacement on Windows.
pub(crate) fn guard_directory(path: &Path) -> Result<Vec<File>, ToolError> {
    let canonical = path.canonicalize()?;
    let mut held = Vec::new();
    #[cfg(windows)]
    {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        if canonical.to_string_lossy().starts_with("\\\\?\\UNC\\") {
            return Err(ToolError::new(
                "UNSUPPORTED_CAPABILITY",
                "A local disk directory is required",
            ));
        }
        for ancestor in canonical.ancestors() {
            let file = OpenOptions::new()
                .read(true)
                .share_mode(3)
                .custom_flags(0x02000000 | 0x00200000)
                .open(ancestor)?;
            if file.metadata()?.file_attributes() & 0x400 != 0 {
                return Err(ToolError::new(
                    "PERMISSION_DENIED",
                    "Directory reparse point rejected",
                ));
            }
            held.push(file);
        }
    }
    #[cfg(not(windows))]
    {
        held.push(File::open(canonical)?);
    }
    Ok(held)
}
pub(crate) fn read_file(path: &Path) -> Result<File, ToolError> {
    no_link(path)?;
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "Only regular files are supported",
        ));
    }
    let mut opts = OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        opts.custom_flags(0x00200000).share_mode(1);
    }
    let file = opts.open(path)?;
    let meta = file.metadata()?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err(ToolError::new(
                "PERMISSION_DENIED",
                "Reparse point rejected",
            ));
        }
    }
    if !meta.is_file() {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "Only regular files are supported",
        ));
    }
    Ok(file)
}
pub(crate) struct Temporary {
    pub(crate) path: PathBuf,
    pub(crate) committed: bool,
}
impl Drop for Temporary {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.path);
        }
    }
}
pub(crate) fn create_temp(dest: &Path) -> Result<(File, Temporary), ToolError> {
    let p = dest.parent().unwrap_or(Path::new("."));
    let path = p.join(format!(".hopkit-{}.part", uuid::Uuid::new_v4()));
    let f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)?;
    Ok((
        f,
        Temporary {
            path,
            committed: false,
        },
    ))
}
pub(crate) fn commit(mut temp: Temporary, dest: &Path, overwrite: bool) -> Result<(), ToolError> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::*;
        let src: Vec<u16> = temp.path.as_os_str().encode_wide().chain(Some(0)).collect();
        let dst: Vec<u16> = dest.as_os_str().encode_wide().chain(Some(0)).collect();
        let flags = MOVEFILE_WRITE_THROUGH
            | if overwrite {
                MOVEFILE_REPLACE_EXISTING
            } else {
                0
            };
        if unsafe { MoveFileExW(src.as_ptr(), dst.as_ptr(), flags) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    #[cfg(not(windows))]
    {
        if overwrite {
            fs::rename(&temp.path, dest)?;
        } else {
            fs::hard_link(&temp.path, dest)?;
            fs::remove_file(&temp.path)?;
        }
    }
    temp.committed = true;
    Ok(())
}

/// Publish a completed file without replacement. A cleanup warning never undoes
/// an already successful publication (including the POSIX hard-link case).
pub(crate) fn publish(mut temp: Temporary, dest: &Path) -> Result<Option<String>, ToolError> {
    #[cfg(windows)]
    {
        commit(temp, dest, false)?;
        Ok(None)
    }
    #[cfg(not(windows))]
    {
        fs::hard_link(&temp.path, dest)?;
        temp.committed = true;
        Ok(fs::remove_file(&temp.path)
            .err()
            .map(|e| format!("Saved; could not remove {}: {e}", temp.path.display())))
    }
}
impl Temporary {
    pub(crate) fn cleanup(&mut self) -> Result<(), std::io::Error> {
        if !self.committed {
            match fs::remove_file(&self.path) {
                Ok(()) => self.committed = true,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => self.committed = true,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}
