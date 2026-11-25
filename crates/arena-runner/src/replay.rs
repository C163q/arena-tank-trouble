use std::error::Error;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::matchplay::Replay;

const PAGE: &str = include_str!("../web/replay.html");
const PLAYER: &str = include_str!("../web/replay.js");
const CANVAS: &str = include_str!("../../arena-playtest/web/canvas.js");

pub fn export(out: &Path, replay: &Replay) -> Result<PathBuf, Box<dyn Error>> {
    let encoded = serde_json::to_vec(replay)?;
    if let Some(parent) = out.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(out)?;
    fs::write(out.join("replay.json"), &encoded)?;
    let path = out.join("replay.html");
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
    file.flush()?;
    Ok(path)
}
