mod controller;
mod matchplay;
#[cfg(target_os = "linux")]
mod native;
mod replay;
mod timeline;

use std::error::Error;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use controller::{Controller, Demo};
use matchplay::MatchConfig;

const USAGE: &str = "Usage: arena-runner --out DIR [--seed N] [--max-ticks N] [--kill-points N] [--survival-points N] BOT BOT [BOT [BOT]]\nBOT: demo or a Linux shared library (.so)\n       arena-runner viewer --out HTML";

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1).peekable();
    match args.peek().and_then(|arg| arg.to_str()) {
        Some("viewer") => {
            args.next();
            if args.next().as_deref() != Some(std::ffi::OsStr::new("--out")) {
                return Err("Usage: arena-runner viewer --out HTML".into());
            }
            let output = args.next().ok_or("missing viewer --out value")?;
            if args.next().is_some() {
                return Err("Usage: arena-runner viewer --out HTML".into());
            }
            replay::export_viewer(Path::new(&output))?;
            println!("{}", Path::new(&output).display());
            return Ok(());
        }
        Some("--help" | "-h") => {
            println!("{USAGE}");
            return Ok(());
        }
        #[cfg(target_os = "linux")]
        Some("--worker") => {
            args.next();
            let path = args.next().ok_or("missing worker library")?;
            let referee = number("--worker referee PID", args.next())?;
            return Ok(native::serve(Path::new(&path), referee)?);
        }
        _ => {}
    }
    let mut config = MatchConfig::default();
    let mut out = None;
    let mut bots = Vec::new();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--out") => out = Some(PathBuf::from(args.next().ok_or("missing --out value")?)),
            Some("--seed") => config.seed = number("--seed", args.next())?,
            Some("--max-ticks") => config.max_ticks = number("--max-ticks", args.next())?,
            Some("--kill-points") => config.kill_points = number("--kill-points", args.next())?,
            Some("--survival-points") => {
                config.survival_points = number("--survival-points", args.next())?;
            }
            Some(flag) if flag.starts_with('-') => {
                return Err(format!("unknown option: {flag}").into());
            }
            _ => bots.push(arg),
        }
    }
    let out = out.ok_or(USAGE)?;
    if !(2..=4).contains(&bots.len()) {
        return Err(format!("provide 2 to 4 bots\n{USAGE}").into());
    }
    if !(1..=36_000).contains(&config.max_ticks) {
        return Err("--max-ticks must be between 1 and 36000".into());
    }
    if out.exists() {
        return Err(format!("output path already exists: {}", out.display()).into());
    }
    let mut controllers = bots
        .into_iter()
        .enumerate()
        .map(|(seat, bot)| load_bot(seat, bot))
        .collect::<Result<Vec<_>, _>>()?;
    let result = matchplay::run(config, &mut controllers)?;
    drop(controllers);
    let file = replay::export(&out, &result)?;
    println!("{}", file.display());
    Ok(())
}

fn number<T: std::str::FromStr>(name: &str, value: Option<OsString>) -> Result<T, String> {
    value
        .and_then(|text| text.to_str()?.parse().ok())
        .ok_or_else(|| format!("invalid value for {name}"))
}

fn load_bot(seat: usize, spec: OsString) -> Result<Box<dyn Controller>, Box<dyn Error>> {
    if spec == "demo" {
        return Ok(Box::new(Demo {
            name: format!("示例 {}", seat + 1),
        }));
    }
    #[cfg(target_os = "linux")]
    {
        let path = PathBuf::from(spec).canonicalize()?;
        let name = path
            .file_stem()
            .unwrap_or(path.as_os_str())
            .to_string_lossy()
            .into_owned();
        let bot = native::Native::new(name, path.clone())
            .map_err(|error| format!("loading bot {}: {error}", path.display()))?;
        Ok(Box::new(bot))
    }
    #[cfg(not(target_os = "linux"))]
    Err("loading native bots requires Linux".into())
}
