//! Each part's result records a hash of its inputs, and a part whose hash equals that of an
//! earlier passing result of the same `--phase` does not run again. The run copies that result
//! and names its source.
//!
//! The inputs are wide, as a missed input is a hole and an extra one only costs time: every
//! oracle file and oracle key of [`crate::oracle`], the lock, the `--phase`, the files
//! [`is_part_input`] selects, untracked ones included, and the [`tool_versions`].

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::cmd::{self, git};
use crate::repo::{entry_hash, sha256};
use crate::results::{Outcome, PartFile};

/// The Rust and Cargo files every part that builds `wtpool` reads.
const RUST_INPUTS: [&str; 7] = [
    "crates/",
    "Cargo.toml",
    "Cargo.lock",
    ".cargo/",
    "rust-toolchain",
    "rust-toolchain.toml",
    "build.rs",
];

/// True when `rel`, a path of the tree, is an input of `part` besides the oracle files.
pub fn is_part_input(part: &str, rel: &str) -> bool {
    let under = |prefix: &str| {
        if let Some(dir) = prefix.strip_suffix('/') {
            rel == dir || rel.starts_with(prefix)
        } else {
            rel == prefix
        }
    };
    let rust = RUST_INPUTS.iter().any(|p| under(p));
    match part {
        "quick" => !under("docs/") && !under("verify/reviews/") && !under("verify/out/"),
        "shot" | "ui" => rust || under("vscode/"),
        "int" | "bench" | "cc" | "oc" => rust,
        _ => true,
    }
}

/// Every file of the tree, tracked or untracked but not ignored, relative to `root`.
pub fn tree_files(root: &Path) -> Result<Vec<String>, String> {
    let listed = git(
        root,
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
    )?;
    let mut files: Vec<String> = listed
        .split('\0')
        .filter(|rel| !rel.is_empty())
        .map(str::to_owned)
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
}

fn version(program: &str, args: &[&str]) -> String {
    cmd::output(Command::new(program).args(args), Duration::from_secs(30))
        .map(|text| text.trim().to_owned())
        .unwrap_or_else(|_| format!("{program}: not found"))
}

/// The VS Code that vscode/test downloads when none is installed: `PINNED_VERSION` of
/// vscode/test/support/vscode-app.mjs, the oldest the extension supports (`engines.vscode`).
const PINNED_VSCODE: &str = "1.140.0";

/// The VS Code executable `WTPOOL_VERIFY_VSCODE` names, the first choice of vscode/test.
fn named_vscode() -> Option<PathBuf> {
    std::env::var_os("WTPOOL_VERIFY_VSCODE")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// The `code` command line of the VS Code executable `app` on the system `os`
/// ([`std::env::consts::OS`]), where @vscode/test-electron's
/// `resolveCliPathFromVSCodeExecutablePath` puts it.
fn cli_of_app(app: &Path, os: &str) -> PathBuf {
    let up = |levels: usize| app.ancestors().nth(levels).unwrap_or(Path::new(""));
    let insiders = |suffix: &str| app.to_string_lossy().ends_with(suffix);
    match os {
        "windows" if insiders("Code - Insiders.exe") => up(1).join("bin/code-insiders.cmd"),
        "windows" => up(1).join("bin/code.cmd"),
        "macos" => up(3).join("Contents/Resources/app/bin/code"),
        _ if insiders("code-insiders") => up(1).join("bin/code-insiders"),
        _ => up(1).join("bin/code"),
    }
}

/// The VS Code command line, as vscode/test finds it: `WTPOOL_VERIFY_VSCODE_CLI`, else that of
/// the executable `WTPOOL_VERIFY_VSCODE` names, else where VS Code installs it on this system,
/// else the pinned VS Code downloaded into `<cache>/wtpool-verify/vscode/`. `None` when none
/// exists.
pub fn vscode_cli() -> Option<PathBuf> {
    if let Some(named) = std::env::var_os("WTPOOL_VERIFY_VSCODE_CLI").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(named));
    }
    if let Some(app) = named_vscode() {
        return Some(cli_of_app(&app, std::env::consts::OS));
    }
    let installed: Option<PathBuf> = if cfg!(target_os = "macos") {
        Some(PathBuf::from(
            "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code",
        ))
    } else if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(|dir| PathBuf::from(dir).join("Programs/Microsoft VS Code/bin/code.cmd"))
    } else {
        Some(PathBuf::from("/usr/share/code/bin/code"))
    };
    if let Some(path) = installed.filter(|path| path.is_file()) {
        return Some(path);
    }
    let on_path = std::env::var_os("PATH")
        .filter(|_| !cfg!(any(target_os = "macos", windows)))
        .and_then(|path| {
            std::env::split_paths(&path)
                .map(|dir| dir.join("code"))
                .find(|candidate| candidate.is_file())
        });
    on_path.or_else(|| {
        let cache = crate::dirs::cache_dir().ok()?;
        downloaded_vscode_cli(&cache.join("vscode"))
    })
}

/// The command line of the pinned VS Code that @vscode/test-electron downloaded into
/// `downloads`, in `vscode-<platform>-<version>/`, once its `is-complete` marker says the download
/// finished.
fn downloaded_vscode_cli(downloads: &Path) -> Option<PathBuf> {
    let dir = downloads.join(format!("vscode-{}-{PINNED_VSCODE}", vscode_platform()));
    let cli = if cfg!(target_os = "macos") {
        dir.join("Visual Studio Code.app/Contents/Resources/app/bin/code")
    } else if cfg!(windows) {
        dir.join("bin/code.cmd")
    } else {
        dir.join("bin/code")
    };
    (dir.join("is-complete").is_file() && cli.is_file()).then_some(cli)
}

/// @vscode/test-electron's name of this platform, as in its download folders.
fn vscode_platform() -> &'static str {
    let arm64 = cfg!(target_arch = "aarch64");
    if cfg!(target_os = "macos") {
        if arm64 { "darwin-arm64" } else { "darwin" }
    } else if cfg!(windows) {
        if arm64 {
            "win32-arm64-archive"
        } else {
            "win32-x64-archive"
        }
    } else if arm64 {
        "linux-arm64"
    } else if cfg!(target_arch = "arm") {
        "linux-armhf"
    } else {
        "linux-x64"
    }
}

/// The installed `opencode` by its resolved path, size and modification time. It never runs:
/// any `opencode` command outside the oc layer's sandbox would write into the owner's OpenCode
/// folders.
fn opencode_identity() -> String {
    let found = cmd::output(
        Command::new("/usr/bin/which").arg("opencode"),
        Duration::from_secs(10),
    );
    let Ok(text) = found else {
        return "opencode: not found".to_owned();
    };
    let path = fs::canonicalize(text.trim()).unwrap_or_else(|_| PathBuf::from(text.trim()));
    match fs::metadata(&path) {
        Ok(meta) => {
            let modified = meta
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |since| since.as_secs());
            format!(
                "{} {} bytes, modified {modified}",
                path.display(),
                meta.len()
            )
        }
        Err(error) => format!("{}: {error}", path.display()),
    }
}

/// The tool versions every part's hash holds.
pub fn tool_versions() -> BTreeMap<String, String> {
    let cli = vscode_cli();
    let vscode = match &cli {
        Some(code) => version(&code.to_string_lossy(), &["--version"]),
        None => "code: not found".to_owned(),
    };
    let mut versions = BTreeMap::from([
        ("rustc".to_owned(), version("rustc", &["-V"])),
        ("cargo".to_owned(), version("cargo", &["-V"])),
        ("node".to_owned(), version("node", &["-v"])),
        ("vscode".to_owned(), vscode.clone()),
        ("claude".to_owned(), version("claude", &["--version"])),
        ("opencode".to_owned(), opencode_identity()),
    ]);
    // The tests run this executable even when `WTPOOL_VERIFY_VSCODE_CLI` names another VS Code's
    // command line, so its own command line reports its version.
    if let Some(app) = named_vscode() {
        let own = cli_of_app(&app, std::env::consts::OS);
        let version = if cli.as_ref() == Some(&own) {
            vscode
        } else {
            version(&own.to_string_lossy(), &["--version"])
        };
        versions.insert(
            "vscode_app".to_owned(),
            format!("{}\n{version}", app.display()),
        );
    }
    versions
}

/// The inputs of `part` at `phase`, each with its hash.
pub fn inputs(
    root: &Path,
    phase: u8,
    part: &str,
    versions: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, String> {
    let mut found: BTreeMap<String, String> = BTreeMap::new();
    found.insert("phase".to_owned(), phase.to_string());
    found.insert("part".to_owned(), part.to_owned());
    for (name, value) in versions {
        found.insert(format!("version:{name}"), value.clone());
    }
    for (rel, hash) in crate::oracle::oracle_files(root, phase) {
        found.insert(format!("oracle:{rel}"), hash);
    }
    for (key, hash) in crate::oracle::oracle_keys(root) {
        found.insert(format!("key:{key}"), hash);
    }
    found.insert(
        format!("oracle:{}", crate::oracle::LOCK),
        entry_hash(&root.join(crate::oracle::LOCK)),
    );
    for rel in tree_files(root)? {
        if is_part_input(part, &rel) {
            found.insert(format!("file:{rel}"), entry_hash(&root.join(&rel)));
        }
    }
    Ok(found)
}

/// A hash of `HEAD` and of the files [`is_part_input`] selects for `part`, without the oracle
/// files: the inputs of the part's build, made once per run.
pub fn build_hash(root: &Path, part: &str) -> Result<String, String> {
    let mut found = BTreeMap::new();
    for rel in tree_files(root)? {
        if is_part_input(part, &rel) {
            found.insert(rel.clone(), entry_hash(&root.join(&rel)));
        }
    }
    found.insert(
        "head".to_owned(),
        crate::repo::head(root).unwrap_or_default(),
    );
    Ok(hash_of(&found))
}

pub fn hash_of(inputs: &BTreeMap<String, String>) -> String {
    let mut text = String::new();
    for (name, hash) in inputs {
        text.push_str(name);
        text.push('\t');
        text.push_str(hash);
        text.push('\n');
    }
    sha256(text.as_bytes())
}

pub fn input_hash(root: &Path, phase: u8, part: &str) -> Result<String, String> {
    Ok(hash_of(&inputs(root, phase, part, &tool_versions())?))
}

/// Each `parts/<part>.json` below `dir`, at any depth.
fn part_files(dir: &Path, part: &str, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            part_files(&path, part, found);
        } else if path
            .file_name()
            .is_some_and(|n| n == format!("{part}.json").as_str())
            && path
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|n| n == "parts")
        {
            found.push(path);
        }
    }
}

/// True when every result of `file` passed. A skip with the code
/// [`NO_RECORDINGS`](crate::scrub::NO_RECORDINGS), such as the fresh scrub or the live sandbox
/// session of a replay only `cc` part, counts as passed while the recordings folder is still
/// missing. The folder is no input of the hash, so on a machine that has gained it the check runs
/// again rather than copy a skip. Any other skip blocks reuse.
pub fn passed(file: &PartFile) -> bool {
    passed_with(file, crate::scrub::recordings_missing())
}

fn passed_with(file: &PartFile, recordings_missing: bool) -> bool {
    !file.results.is_empty()
        && file.results.iter().all(|r| {
            r.outcome == Outcome::Pass
                || (r.outcome == Outcome::Skipped
                    && recordings_missing
                    && r.code.as_deref() == Some(crate::scrub::NO_RECORDINGS))
        })
}

/// An earlier passing result of `part` at `phase` with input hash `hash`, outside `run_dir`.
pub fn earlier_result(
    root: &Path,
    run_dir: &Path,
    part: &str,
    phase: u8,
    hash: &str,
) -> Option<(PathBuf, PartFile)> {
    let mut candidates = Vec::new();
    part_files(&root.join("verify/out"), part, &mut candidates);
    part_files(&root.join("verify/reviews"), part, &mut candidates);
    candidates.sort();
    candidates
        .into_iter()
        .filter(|path| !path.starts_with(run_dir))
        .filter_map(|path| {
            let text = fs::read_to_string(&path).ok()?;
            let file: PartFile = serde_json::from_str(&text).ok()?;
            Some((path, file))
        })
        .find(|(_, file)| {
            file.phase == phase
                && file.part == part
                && file.input_hash.as_deref() == Some(hash)
                && passed(file)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    /// The @vscode/test-electron of vscode/test/package-lock.json, whose
    /// `resolveCliPathFromVSCodeExecutablePath` [`cli_of_app`] follows.
    const TEST_ELECTRON: &str = "3.1.0";

    /// A repository with one untracked file of each kind.
    fn tree(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("xtask-reuse-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        for (rel, text) in [
            ("crates/wtpool/src/main.rs", "fn main() {}\n"),
            ("Cargo.toml", "[workspace]\n"),
            ("Cargo.lock", "# lock\n"),
            ("vscode/src/extension.ts", "export {};\n"),
            ("vscode/package.json", "{}\n"),
            ("docs/design/note.md", "spec\n"),
            ("docs/other.md", "notes\n"),
            ("verify/reviews/phase-1/review.md", "clean\n"),
            ("README.md", "readme\n"),
            ("xtask/src/lib.rs", "// oracle\n"),
        ] {
            let path = dir.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        crate::pool::git_in(&dir, &["init", "--quiet"]).unwrap();
        dir
    }

    fn part_with(results: Vec<crate::results::CheckResult>) -> PartFile {
        PartFile {
            part: "quick".to_owned(),
            phase: 2,
            commit: "c".to_owned(),
            results,
            input_hash: None,
            reused: None,
        }
    }

    #[rstest]
    #[case::all_pass(None, true, true)]
    #[case::no_recordings_still_missing(Some(Some(crate::scrub::NO_RECORDINGS)), true, true)]
    #[case::no_recordings_now_present(Some(Some(crate::scrub::NO_RECORDINGS)), false, false)]
    #[case::other_skip(Some(None), true, false)]
    fn passed__should_count_only_a_skip_for_missing_recordings(
        #[case] skip: Option<Option<&str>>,
        #[case] missing: bool,
        #[case] expected: bool,
    ) {
        // Given
        use crate::results::CheckResult;
        let mut results = vec![CheckResult::pass("a", "static", "")];
        if let Some(code) = skip {
            let mut skipped = CheckResult::skipped("b", "static", "no input");
            skipped.code = code.map(str::to_owned);
            results.push(skipped);
        }

        // When
        let found = passed_with(&part_with(results), missing);

        // Then
        assert_eq!(found, expected);
        assert!(!passed_with(&part_with(Vec::new()), missing));
    }

    fn hash(dir: &Path, part: &str) -> String {
        let versions = BTreeMap::from([("rustc".to_owned(), "rustc 1".to_owned())]);
        hash_of(&inputs(dir, 2, part, &versions).unwrap())
    }

    #[rstest]
    #[case::int_reads_crates("int", "crates/wtpool/src/main.rs", true)]
    #[case::int_reads_cargo_lock("int", "Cargo.lock", true)]
    #[case::int_reads_the_oracle("int", "xtask/src/lib.rs", true)]
    #[case::int_reads_the_spec("int", "docs/design/note.md", true)]
    #[case::int_ignores_other_docs("int", "docs/other.md", false)]
    #[case::int_ignores_vscode("int", "vscode/src/extension.ts", false)]
    #[case::int_ignores_reviews("int", "verify/reviews/phase-1/review.md", false)]
    #[case::bench_reads_crates("bench", "crates/wtpool/src/main.rs", true)]
    #[case::bench_ignores_vscode("bench", "vscode/src/extension.ts", false)]
    #[case::bench_reads_cargo_toml("bench", "Cargo.toml", true)]
    #[case::bench_ignores_readme("bench", "README.md", false)]
    #[case::ui_reads_vscode("ui", "vscode/src/extension.ts", true)]
    #[case::ui_reads_crates("ui", "crates/wtpool/src/main.rs", true)]
    #[case::ui_ignores_other_docs("ui", "docs/other.md", false)]
    #[case::shot_reads_vscode("shot", "vscode/src/extension.ts", true)]
    #[case::shot_reads_crates("shot", "crates/wtpool/src/main.rs", true)]
    #[case::shot_ignores_other_docs("shot", "docs/other.md", false)]
    #[case::quick_reads_readme("quick", "README.md", true)]
    #[case::quick_reads_vscode("quick", "vscode/package.json", true)]
    #[case::quick_ignores_other_docs("quick", "docs/other.md", false)]
    #[case::quick_ignores_reviews("quick", "verify/reviews/phase-1/review.md", false)]
    #[case::cc_reads_crates("cc", "crates/wtpool/src/main.rs", true)]
    #[case::cc_ignores_vscode_src("cc", "vscode/src/extension.ts", false)]
    fn input_hash__should_change_with_each_input_only(
        #[case] part: &str,
        #[case] changed: &str,
        #[case] input: bool,
    ) {
        // Given
        let dir = tree(&format!("{part}-{}", changed.replace('/', "-")));
        let before = hash(&dir, part);

        // When
        fs::write(dir.join(changed), "changed\n").unwrap();
        let after = hash(&dir, part);

        // Then
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(before != after, input, "{part} after a change of {changed}");
    }

    #[test]
    fn input_hash__should_change_with_a_tool_version_and_the_phase() {
        // Given
        let dir = tree("versions");
        let one = BTreeMap::from([("claude".to_owned(), "2.1.306".to_owned())]);
        let two = BTreeMap::from([("claude".to_owned(), "2.1.307".to_owned())]);

        // When
        let base = hash_of(&inputs(&dir, 2, "cc", &one).unwrap());
        let other_version = hash_of(&inputs(&dir, 2, "cc", &two).unwrap());
        let other_phase = hash_of(&inputs(&dir, 3, "cc", &one).unwrap());

        // Then
        let _ = fs::remove_dir_all(&dir);
        assert_ne!(base, other_version);
        assert_ne!(base, other_phase);
    }

    #[test]
    fn downloaded_vscode_cli__should_find_a_finished_download_of_the_pinned_version() {
        // Given
        let downloads = std::env::temp_dir().join(format!("xtask-vscode-{}", std::process::id()));
        let _ = fs::remove_dir_all(&downloads);
        let dir = downloads.join(format!("vscode-{}-1.140.0", vscode_platform()));
        let cli = dir.join(if cfg!(target_os = "macos") {
            "Visual Studio Code.app/Contents/Resources/app/bin/code"
        } else if cfg!(windows) {
            "bin/code.cmd"
        } else {
            "bin/code"
        });
        fs::create_dir_all(cli.parent().unwrap()).unwrap();
        fs::write(&cli, "").unwrap();

        // When
        let unfinished = downloaded_vscode_cli(&downloads);
        fs::write(dir.join("is-complete"), "").unwrap();
        let finished = downloaded_vscode_cli(&downloads);

        // Then
        let _ = fs::remove_dir_all(&downloads);
        assert_eq!(unfinished, None);
        assert_eq!(finished, Some(cli));
    }

    #[test]
    fn pinned_vscode__should_be_the_version_vscode_test_downloads() {
        // Given
        let app =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../vscode/test/support/vscode-app.mjs");

        // When
        let text = fs::read_to_string(app).unwrap();

        // Then
        let pinned = format!("export const PINNED_VERSION = '{PINNED_VSCODE}';");
        assert!(text.contains(&pinned), "{pinned}");
    }

    #[test]
    fn cache_dir__should_be_the_cache_root_of_the_js_tests() {
        // Given
        let test = Path::new(env!("CARGO_MANIFEST_DIR")).join("../vscode/test");
        let app = fs::read_to_string(test.join("support/vscode-app.mjs")).unwrap();
        let code = fs::read_to_string(test.join("support/code.mjs")).unwrap();

        // When
        let cache = crate::dirs::cache_dir().unwrap();

        // Then
        let xdg = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .filter(|dir| dir.is_absolute());
        let base = xdg.unwrap_or_else(|| crate::dirs::home().unwrap().join(".cache"));
        let name = cache.strip_prefix(&base).unwrap().to_str().unwrap();
        for line in [
            "const base = xdg && path.isAbsolute(xdg) ? xdg : path.join(os.homedir(), '.cache');"
                .to_owned(),
            format!("return path.join(base, '{name}');"),
            "return path.join(cacheRoot(), 'vscode');".to_owned(),
        ] {
            assert!(app.contains(&line), "vscode-app.mjs no longer has {line}");
        }
        let links = "const LINKS = path.join(cacheRoot(), 's');";
        assert!(code.contains(links), "code.mjs no longer has {links}");
        let dirs = include_str!("dirs.rs");
        assert!(
            dirs.contains(r#"pub const SHORT_LINKS: &str = "s";"#),
            "dirs.rs no longer defines SHORT_LINKS as \"s\""
        );
        let js = include_str!("layers/js.rs");
        assert!(
            js.contains("crate::dirs::SHORT_LINKS"),
            "layers/js.rs no longer uses SHORT_LINKS"
        );
    }

    #[rstest]
    #[case::macos(
        "macos",
        "/A/Visual Studio Code.app/Contents/MacOS/Electron",
        "/A/Visual Studio Code.app/Contents/Resources/app/bin/code"
    )]
    #[case::linux("linux", "/opt/code/code", "/opt/code/bin/code")]
    #[case::linux_insiders(
        "linux",
        "/opt/insiders/code-insiders",
        "/opt/insiders/bin/code-insiders"
    )]
    #[case::windows("windows", "C:/VS Code/Code.exe", "C:/VS Code/bin/code.cmd")]
    #[case::windows_insiders(
        "windows",
        "C:/Insiders/Code - Insiders.exe",
        "C:/Insiders/bin/code-insiders.cmd"
    )]
    fn cli_of_app__should_find_the_command_line_of_an_executable(
        #[case] os: &str,
        #[case] app: &str,
        #[case] cli: &str,
    ) {
        // When
        let found = cli_of_app(Path::new(app), os);

        // Then
        assert_eq!(found, Path::new(cli));
    }

    #[test]
    fn vscode_cli__should_follow_the_discovery_of_vscode_test() {
        // Given
        let test = Path::new(env!("CARGO_MANIFEST_DIR")).join("../vscode/test");

        // When
        let app = fs::read_to_string(test.join("support/vscode-app.mjs")).unwrap();
        let lock: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(test.join("package-lock.json")).unwrap())
                .unwrap();

        // Then
        for line in [
            "const fromEnv = process.env.WTPOOL_VERIFY_VSCODE;",
            "const cliFromEnv = process.env.WTPOOL_VERIFY_VSCODE_CLI;",
            "if (fromEnv) {",
            "return { app: fromEnv, cli: cliFromEnv ?? resolveCliPathFromVSCodeExecutablePath(fromEnv), source: 'WTPOOL_VERIFY_VSCODE' };",
        ] {
            assert!(app.contains(line), "vscode-app.mjs no longer has {line}");
        }
        let locked = &lock["packages"]["node_modules/@vscode/test-electron"]["version"];
        assert_eq!(
            locked.as_str(),
            Some(TEST_ELECTRON),
            "check cli_of_app against its util.js"
        );
    }

    #[test]
    fn input_hash__should_count_an_untracked_file() {
        // Given
        let dir = tree("untracked");
        let before = hash(&dir, "int");

        // When
        fs::write(dir.join("crates/wtpool/src/new.rs"), "// new\n").unwrap();
        let after = hash(&dir, "int");

        // Then
        let _ = fs::remove_dir_all(&dir);
        assert_ne!(before, after);
    }
}
