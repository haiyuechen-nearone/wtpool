//! The output other programs read from `wtpool`: the schema of the status document and the hook
//! block for Claude Code's settings. Every run gets a temporary `HOME` and pool.
#![cfg(unix)]
// Always true here. It lets clippy's `*-in-tests` settings cover the helpers too.
#![cfg(test)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A temporary folder that serves as `HOME`, foreground and slot folder, none of them a
/// repository.
struct Sandbox {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Sandbox {
    fn must_new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        Self { _dir: dir, root }
    }

    /// `binary` with `args` and only `env` besides the sandbox's own variables.
    fn must_run(&self, binary: &Path, args: &[&str], env: &[(&str, &Path)]) -> Output {
        Command::new(binary)
            .args(args)
            .current_dir(&self.root)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &self.root)
            .env("WTPOOL_FOREGROUND", self.root.join("app"))
            .env("WTPOOL_SLOT_DIR", &self.root)
            // A coverage run also counts the lines the spawned binary runs.
            .envs(std::env::var_os("LLVM_PROFILE_FILE").map(|file| ("LLVM_PROFILE_FILE", file)))
            .envs(env.iter().copied())
            .output()
            .unwrap()
    }

    /// A copy of `wtpool` outside `target`, where `wtpool hook install` accepts it.
    fn must_install_wtpool(&self) -> PathBuf {
        let bin = self.root.join("bin");
        std::fs::create_dir(&bin).unwrap();
        let installed = bin.join("wtpool");
        std::fs::copy(env!("CARGO_BIN_EXE_wtpool"), &installed).unwrap();
        installed
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[test]
fn status_schema__should_print_the_schema_without_a_repository() {
    // Given
    let sandbox = Sandbox::must_new();

    // When
    let output = sandbox.must_run(
        Path::new(env!("CARGO_BIN_EXE_wtpool")),
        &["status", "--schema"],
        &[],
    );

    // Then
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        text(&output.stdout),
        wtpool_core::status::status_schema_text()
    );
    assert_eq!(text(&output.stderr), "");
}

#[test]
fn hook_install__should_check_the_settings_in_claude_config_dir() {
    // Given
    let sandbox = Sandbox::must_new();
    let wtpool = sandbox.must_install_wtpool();
    let config = sandbox.root.join("claude-config");
    std::fs::create_dir(&config).unwrap();
    let with_config = [("CLAUDE_CONFIG_DIR", config.as_path())];

    // When
    let before = sandbox.must_run(&wtpool, &["hook", "install"], &with_config);
    std::fs::write(config.join("settings.json"), &before.stdout).unwrap();
    let after = sandbox.must_run(&wtpool, &["hook", "install"], &with_config);
    let in_home = sandbox.must_run(&wtpool, &["hook", "install"], &[]);

    // Then
    let shown = config.join("settings.json").display().to_string();
    assert!(
        before.status.success() && after.status.success(),
        "{before:?}"
    );
    assert_eq!(
        text(&before.stderr),
        format!(
            "hooks missing: {shown} is missing or cannot be read.\nInstall the plugin \
             wtpool@wtpool from the marketplace haiyuechen-nearone/wtpool, or merge this block \
             into {shown} yourself: wtpool never writes that file.\n"
        )
    );
    assert_eq!(
        text(&after.stderr),
        format!("hooks present: {shown} holds this block.\n")
    );
    assert_eq!(before.stdout, after.stdout);
    assert!(
        text(&in_home.stderr)
            .starts_with("hooks missing: ~/.claude/settings.json is missing or cannot be read.")
    );
    assert!(!sandbox.root.join(".claude").exists());
}

#[test]
fn hook_install__should_count_the_enabled_plugin() {
    // Given
    let sandbox = Sandbox::must_new();
    let wtpool = sandbox.must_install_wtpool();
    let config = sandbox.root.join("claude-config");
    std::fs::create_dir(&config).unwrap();
    let settings = config.join("settings.json");
    let bin = sandbox.root.join("bin");
    let install = |path: &Path| {
        sandbox.must_run(
            &wtpool,
            &["hook", "install"],
            &[("CLAUDE_CONFIG_DIR", config.as_path()), ("PATH", path)],
        )
    };

    // When
    std::fs::write(&settings, r#"{"enabledPlugins": {"wtpool@wtpool": true}}"#).unwrap();
    let on_path = install(&bin);
    let off_path = install(Path::new("/usr/bin"));
    let both =
        text(&on_path.stdout).replacen('{', r#"{"enabledPlugins": {"wtpool@wtpool": true},"#, 1);
    std::fs::write(&settings, both).unwrap();
    let twice = install(&bin);

    // Then
    let shown = settings.display().to_string();
    assert_eq!(
        text(&on_path.stderr),
        format!(
            "hooks present: {shown} enables the plugin wtpool@wtpool.\nIts hooks run the wtpool on \
             PATH, here {}.\n",
            wtpool.display()
        )
    );
    assert!(
        text(&off_path.stderr).ends_with(
            "and no wtpool is on PATH here: put wtpool there, in the PATH Claude Code starts \
             with.\n"
        ),
        "{off_path:?}"
    );
    assert!(
        text(&twice.stderr).starts_with(&format!(
            "hooks present twice: {shown} enables the plugin wtpool@wtpool and holds a wtpool hook \
             block"
        )),
        "{twice:?}"
    );
    assert_eq!(on_path.stdout, twice.stdout);
}
