//! The probe command of the job checks (jobs.md, "Verification plan", test interface 3): the
//! command a `wtpool job` runs. Copied under a program's name, such as `cargo` or `go`, it takes
//! any arguments, so a check can give a job exactly the command line it wants, and reads what it
//! does from variables, which a job passes through untouched (J2):
//!
//! - `WTPOOL_PROBE_MARKS`: a folder. At its start the probe writes `<label>.start` there, a JSON
//!   object of its `pid`, the wall clock `at` in nanoseconds since the epoch, its `args` after the
//!   program, its folder `cwd`, `job` (the value of `WTPOOL_JOB`, else null) and `tty`, whether
//!   stdin, stdout and stderr are terminals; at its end `<label>.end` with `pid` and `at`. Each
//!   file appears whole, through a rename.
//! - `WTPOOL_PROBE_LABEL`: the label of those files, else the pid.
//! - `WTPOOL_PROBE_WAIT`: a file the probe waits for, polling every 5 ms, before it ends.
//! - `WTPOOL_PROBE_STDIN=copy`: copies stdin to stdout and to stderr, byte for byte, to its end.
//! - `WTPOOL_PROBE_EXIT`: the exit code, 0 by default.
//! - `WTPOOL_PROBE_SIGNAL`: a signal name such as `TERM`, which the probe sends itself instead of
//!   exiting, through `kill`; exit code 70 if it outlives it.
//!
//! A program rather than a shell script: no shell reads the clock below the second or tells a
//! terminal apart the same way on every system.

use std::ffi::OsString;
use std::fs;
use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn var(name: &str) -> Option<String> {
    std::env::var_os(name).map(|value| value.to_string_lossy().into_owned())
}

/// Writes `text` to `dir/name` through a temporary file, so a reader never sees half of it.
fn write_whole(dir: &Path, name: &str, text: &str) {
    let partial = dir.join(format!(".{name}.partial"));
    if fs::write(&partial, text).is_ok() {
        let _ = fs::rename(&partial, dir.join(name));
    }
}

fn json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn start_mark(args: &[OsString]) -> String {
    let words: Vec<String> = args
        .iter()
        .map(|arg| json_string(&arg.to_string_lossy()))
        .collect();
    let cwd = std::env::current_dir().unwrap_or_default();
    let job = var("WTPOOL_JOB").map_or_else(|| "null".to_owned(), |id| json_string(&id));
    format!(
        "{{\"pid\":{},\"at\":{},\"args\":[{}],\"cwd\":{},\"job\":{},\"tty\":[{},{},{}]}}\n",
        std::process::id(),
        now_nanos(),
        words.join(","),
        json_string(&cwd.to_string_lossy()),
        job,
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
        std::io::stderr().is_terminal(),
    )
}

fn copy_stdin() {
    let mut stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();
    let mut buffer = [0_u8; 8192];
    loop {
        match stdin.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                let chunk = buffer.get(..read).unwrap_or_default();
                let _ = stdout.write_all(chunk);
                let _ = stderr.write_all(chunk);
            }
        }
    }
    let _ = stdout.flush();
    let _ = stderr.flush();
}

fn wait_for(file: &Path) {
    while !file.exists() {
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let marks = var("WTPOOL_PROBE_MARKS").map(PathBuf::from);
    let label = var("WTPOOL_PROBE_LABEL").unwrap_or_else(|| std::process::id().to_string());
    if let Some(dir) = &marks {
        write_whole(dir, &format!("{label}.start"), &start_mark(&args));
    }
    if var("WTPOOL_PROBE_STDIN").as_deref() == Some("copy") {
        copy_stdin();
    }
    if let Some(file) = var("WTPOOL_PROBE_WAIT") {
        wait_for(Path::new(&file));
    }
    if let Some(dir) = &marks {
        let end = format!(
            "{{\"pid\":{},\"at\":{}}}\n",
            std::process::id(),
            now_nanos()
        );
        write_whole(dir, &format!("{label}.end"), &end);
    }
    if let Some(signal) = var("WTPOOL_PROBE_SIGNAL") {
        let _ = Command::new("kill")
            .args(["-s", &signal, &std::process::id().to_string()])
            .status();
        std::thread::sleep(Duration::from_secs(5));
        return ExitCode::from(70);
    }
    let code = var("WTPOOL_PROBE_EXIT")
        .and_then(|text| text.parse::<u8>().ok())
        .unwrap_or(0);
    ExitCode::from(code)
}
