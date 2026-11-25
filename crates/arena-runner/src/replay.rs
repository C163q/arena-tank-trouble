use std::error::Error;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::matchplay::Replay;

const PAGE: &str = include_str!("../web/replay.html");
const PLAYER: &str = include_str!("../web/replay.js");
const CANVAS: &str = include_str!("../../arena-playtest/web/canvas.js");

pub fn export(out: &Path, replay: &Replay) -> Result<PathBuf, Box<dyn Error>> {
    let encoded = serde_json::to_vec(replay)?;
    let parent = out
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let stage = parent.join(format!(".arena-replay-{}-{nonce}", std::process::id()));
    fs::create_dir(&stage)?;
    let result = write_files(&stage, &encoded).and_then(|()| publish(&stage, out));
    if result.is_err() {
        for file in ["replay.json", "replay.html"] {
            let _ = fs::remove_file(stage.join(file));
        }
        let _ = fs::remove_dir(&stage);
    }
    result?;
    Ok(out.join("replay.html"))
}

fn write_files(directory: &Path, encoded: &[u8]) -> io::Result<()> {
    fs::write(directory.join("replay.json"), encoded)?;
    let path = directory.join("replay.html");
    let (head, tail) = PAGE
        .split_once("<!-- REPLAY_SCRIPT -->")
        .expect("script marker");
    let mut file = BufWriter::new(File::create(&path)?);
    write!(
        file,
        "{head}<script type=\"application/json\" id=\"replay-data\">"
    )?;
    for part in encoded.split_inclusive(|&byte| byte == b'<') {
        if let Some(literal) = part.strip_suffix(b"<") {
            file.write_all(literal)?;
            file.write_all(b"\\u003c")?;
        } else {
            file.write_all(part)?;
        }
    }
    write!(
        file,
        "</script>\n<script type=\"module\">\n{CANVAS}\n{PLAYER}\n</script>{tail}"
    )?;
    file.flush()
}

#[cfg(target_os = "linux")]
fn publish(stage: &Path, out: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let stage = CString::new(stage.as_os_str().as_bytes())?;
    let out = CString::new(out.as_os_str().as_bytes())?;
    // SAFETY: both paths are live C strings, relative to the working directory.
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            stage.as_ptr(),
            libc::AT_FDCWD,
            out.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn publish(stage: &Path, out: &Path) -> io::Result<()> {
    if out.try_exists()? {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "output already exists",
        ));
    }
    fs::rename(stage, out)
}
