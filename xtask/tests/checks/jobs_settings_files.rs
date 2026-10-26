//! The files the settings commands write: atomically, privately and in place, editing only the
//! changed value, and refusing a symlink, bad TOML or a comment inside the array they would
//! replace.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};

use crate::support::jobs::describe;
use crate::support::jobs::settings::{
    file, job, jobs_table, mode_of, must_lone_pool, names_path, refused, set_mode, stderr_of,
    strings, succeeded, table,
};
use crate::support::{Sandbox, must_symlink, must_wtpool_bin};

/// Starts `wtpool job <args>` once per item of `patterns` at the same time and waits for each.
fn parallel(sandbox: &Sandbox, cwd: &Path, args: &[&str], patterns: &[String]) -> Vec<Output> {
    let children: Vec<_> = patterns
        .iter()
        .map(|pattern| {
            let mut command = sandbox.command(&must_wtpool_bin(), cwd);
            command
                .arg("job")
                .args(args)
                .arg(pattern)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            command.spawn().expect("setup: start wtpool job heavy add")
        })
        .collect();
    children
        .into_iter()
        .map(|child| child.wait_with_output().expect("setup: wait for wtpool"))
        .collect()
}

/// `file` and `folder` modes in octal, for a message.
fn octal(file: Option<u32>, folder: Option<u32>) -> String {
    let show = |mode: Option<u32>| mode.map_or_else(|| "none".to_owned(), |m| format!("{m:04o}"));
    format!("file {} in folder {}", show(file), show(folder))
}

#[cfg(unix)]
fn inode(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    fs::symlink_metadata(path).ok().map(|meta| meta.ino())
}

#[cfg(not(unix))]
fn inode(_path: &Path) -> Option<u64> {
    None
}

#[test]
fn job_settings__should_write_atomically_and_privately() {
    // Given
    let sandbox = Sandbox::must_new("job-settings-atomic");
    let pool = must_lone_pool(&sandbox);
    let path = sandbox.machine_config();
    let folder: PathBuf = path.parent().expect("setup: a parent").to_path_buf();
    fs::remove_dir_all(&folder).expect("setup: remove the machine config folder");
    let config = pool.common.join("wtpool.toml");
    let mut wrong = Vec::new();

    // When
    let created = job(&sandbox, &pool.fg, &["limit", "3"]);
    let new_modes = (mode_of(&folder), mode_of(&path));
    let existing_inode = if created.status.success() {
        set_mode(&path, 0o640);
        inode(&path)
    } else {
        None
    };
    let rewritten = job(&sandbox, &pool.fg, &["limit", "4"]);
    let machine_patterns: Vec<String> = (1..=16).map(|n| format!("p{n:02}")).collect();
    let machine_writers = parallel(
        &sandbox,
        &pool.fg,
        &["heavy", "add", "--"],
        &machine_patterns,
    );
    let repo_patterns: Vec<String> = (1..=16).map(|n| format!("r{n:02}")).collect();
    let repo_writers = parallel(
        &sandbox,
        &pool.fg,
        &["heavy", "add", "--repo", "--"],
        &repo_patterns,
    );

    // Then
    if succeeded(
        &mut wrong,
        "wtpool job limit 3 without machine.toml",
        &created,
    ) && cfg!(unix)
        && new_modes != (Some(0o700), Some(0o600))
    {
        wrong.push(format!(
            "a new machine.toml and its new folder have modes {}, not 0600 in 0700",
            octal(new_modes.1, new_modes.0)
        ));
    }
    if succeeded(&mut wrong, "wtpool job limit 4", &rewritten) {
        if cfg!(unix) && mode_of(&path) != Some(0o640) {
            wrong.push(format!(
                "the rewrite changed the mode 0640 to {}",
                octal(mode_of(&path), None)
            ));
        }
        if cfg!(unix) && inode(&path) == existing_inode {
            wrong.push("the rewrite wrote machine.toml in place, not through a rename".to_owned());
        }
        let limit = jobs_table(&path).and_then(|t| t.get("limit").cloned());
        if limit != Some(toml::Value::from(4)) {
            wrong.push(format!("after limit 4, machine.toml holds {limit:?}"));
        }
        let left: BTreeSet<String> = fs::read_dir(&folder)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name != "machine.toml" && name != "machine.toml.lock")
            .collect();
        if !left.is_empty() {
            wrong.push(format!("the writes left {left:?} beside machine.toml"));
        }
    }
    let failed: Vec<String> = machine_writers
        .iter()
        .chain(&repo_writers)
        .filter(|o| !o.status.success())
        .map(describe)
        .collect();
    if !failed.is_empty() {
        wrong.push(format!(
            "{} parallel writers failed: {:?}",
            failed.len(),
            failed.first()
        ));
    }
    let machine_heavy = strings(jobs_table(&path).as_ref(), "heavy");
    let lost: Vec<&String> = machine_patterns
        .iter()
        .filter(|p| !machine_heavy.contains(*p))
        .collect();
    if !lost.is_empty() {
        wrong.push(format!("16 parallel machine writers lost {lost:?}"));
    }
    let repo = table(&config);
    let repo_heavy = strings(repo.as_ref(), "heavy");
    let lost: Vec<&String> = repo_patterns
        .iter()
        .filter(|p| !repo_heavy.contains(*p))
        .collect();
    if !lost.is_empty() {
        wrong.push(format!("16 parallel --repo writers lost {lost:?}"));
    }
    if !repo
        .as_ref()
        .is_some_and(|t| t.contains_key("slot_dir") && t.contains_key("slot_count"))
    {
        wrong.push(format!(
            "the --repo writers lost other keys of wtpool.toml: {repo:?}"
        ));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// True when `stderr`, with `path` taken out as written and resolved, holds the line number
/// `line`.
fn names_line(stderr: &str, path: &Path, line: u32) -> bool {
    let mut rest = stderr.replace(&*path.to_string_lossy(), "");
    if let Ok(resolved) = fs::canonicalize(path) {
        rest = rest.replace(&*resolved.to_string_lossy(), "");
    }
    rest.split(|c: char| !c.is_ascii_digit())
        .any(|digits| digits == line.to_string())
}

/// A heavy array whose line 7 holds a comment inside the array.
const COMMENTED: &str =
    "# line 1\n[jobs]\n# line 3\n# line 4\n# line 5\nheavy = [\n    \"make\", # the build\n]\n";

#[test]
fn job_settings__should_refuse_a_symlink_or_bad_toml() {
    // Given
    let sandbox = Sandbox::must_new("job-settings-refuse");
    let pool = must_lone_pool(&sandbox);
    let path = sandbox.machine_config();
    let config = pool.common.join("wtpool.toml");
    let original = fs::read_to_string(&config).expect("setup: read wtpool.toml");
    let elsewhere = sandbox.root.join("elsewhere");
    fs::create_dir_all(&elsewhere).expect("setup: create a folder");
    let mut wrong = Vec::new();
    let fg = pool.fg.clone();
    let refuse_with_target =
        |wrong: &mut Vec<String>, what: &str, link: &Path, target: &Path, args: &[&str]| {
            let before = file(target);
            let output = job(&sandbox, &fg, args);
            if output.status.code() != Some(125) || !names_path(&stderr_of(&output), target) {
                wrong.push(format!(
                    "{what}: not exit 125 naming {}: {}",
                    target.display(),
                    describe(&output)
                ));
            }
            if file(target) != before
                || !fs::symlink_metadata(link).is_ok_and(|m| m.file_type().is_symlink())
            {
                wrong.push(format!("{what}: the target or the link changed"));
            }
        };

    // When
    let machine_target = elsewhere.join("machine-real.toml");
    fs::write(&machine_target, "[jobs]\nlimit = 2\n").expect("setup: write");
    sandbox.remove_machine();
    must_symlink(&machine_target, &path);
    refuse_with_target(
        &mut wrong,
        "a symlinked machine.toml",
        &path,
        &machine_target,
        &["limit", "3"],
    );
    fs::remove_file(&path).expect("setup: remove the link");
    let repo_target = elsewhere.join("wtpool-real.toml");
    fs::write(&repo_target, &original).expect("setup: write");
    fs::remove_file(&config).expect("setup: remove wtpool.toml");
    must_symlink(&repo_target, &config);
    refuse_with_target(
        &mut wrong,
        "a symlinked wtpool.toml",
        &config,
        &repo_target,
        &["heavy", "add", "--repo", "--", "make"],
    );
    fs::remove_file(&config).expect("setup: remove the link");
    fs::write(&config, &original).expect("setup: write wtpool.toml");

    sandbox.write_machine("[jobs\nlimit = 2\n");
    let before = file(&path);
    let output = job(&sandbox, &pool.fg, &["limit", "3"]);
    refused(
        &mut wrong,
        "machine.toml that is no TOML",
        &output,
        &path,
        &before,
    );
    fs::write(&config, format!("{original}[broken\n")).expect("setup: write wtpool.toml");
    let before = file(&config);
    let output = job(
        &sandbox,
        &pool.fg,
        &["heavy", "add", "--repo", "--", "make"],
    );
    refused(
        &mut wrong,
        "wtpool.toml that is no TOML",
        &output,
        &config,
        &before,
    );
    sandbox.write_machine(COMMENTED);
    let before = file(&path);
    let output = job(&sandbox, &pool.fg, &["heavy", "add", "--", "just"]);
    refused(
        &mut wrong,
        "a comment inside machine.toml's heavy",
        &output,
        &path,
        &before,
    );
    if !names_path(&stderr_of(&output), &path) || !names_line(&stderr_of(&output), &path, 7) {
        wrong.push(format!(
            "the refusal does not name machine.toml and line 7: {}",
            describe(&output)
        ));
    }
    let output = job(&sandbox, &pool.fg, &["limit", "3"]);
    let kept = fs::read_to_string(&path).unwrap_or_default();
    if !output.status.success() || !kept.contains("\"make\", # the build") {
        wrong.push(format!(
            "an edit of the limit must keep heavy and its comment: {}; the file holds {kept:?}",
            describe(&output)
        ));
    }
    let lines_before = original.lines().count();
    fs::write(
        &config,
        format!("{original}heavy = [\n    \"make\", # ours\n]\n"),
    )
    .expect("setup: write");
    let before = file(&config);
    let output = job(
        &sandbox,
        &pool.fg,
        &["heavy", "add", "--repo", "--", "just"],
    );
    refused(
        &mut wrong,
        "a comment inside wtpool.toml's heavy",
        &output,
        &config,
        &before,
    );
    let line = u32::try_from(lines_before + 2).expect("setup: a line number");
    if !names_path(&stderr_of(&output), &config) || !names_line(&stderr_of(&output), &config, line)
    {
        wrong.push(format!(
            "the refusal does not name wtpool.toml and line {line}: {}",
            describe(&output)
        ));
    }

    for (text, args, key, value) in [
        (
            "[jobs]\nlimit = 99\n",
            ["limit", "4"],
            "limit",
            toml::Value::from(4),
        ),
        (
            "[jobs]\nenforce = \"strict\"\n",
            ["enforce", "off"],
            "enforce",
            toml::Value::from("off"),
        ),
    ] {
        sandbox.write_machine(text);
        let output = job(&sandbox, &pool.fg, &args);
        let stored = jobs_table(&path).and_then(|t| t.get(key).cloned());
        if !output.status.success() || stored != Some(value.clone()) {
            wrong.push(format!(
                "{text:?} must be rewritten by {args:?}: {}; {key} is {stored:?}",
                describe(&output)
            ));
        }
    }

    // Then
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// True when `text` is `prefix`, an array that parses to `items`, then `suffix`.
fn array_between(text: &str, prefix: &str, suffix: &str, items: &[&str]) -> bool {
    let Some(middle) = text
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_suffix(suffix))
    else {
        return false;
    };
    let parsed: Option<toml::Table> = toml::from_str(&format!("x = {middle}")).ok();
    let found: Vec<String> = parsed
        .as_ref()
        .and_then(|t| t.get("x"))
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    parsed.is_some() && found == items.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>()
}

const MACHINE: &str = "# Settings of this machine, by hand.\n[jobs]\n# How many at once.\nlimit = 3   # three\nenforce = \"warn\"\nheavy = [\"make\"]   # the build\n";

#[test]
fn job_settings__should_edit_in_place() {
    // Given
    let sandbox = Sandbox::must_new("job-settings-in-place");
    let pool = must_lone_pool(&sandbox);
    let path = sandbox.machine_config();
    let config = pool.common.join("wtpool.toml");
    let original = fs::read_to_string(&config).expect("setup: read wtpool.toml");
    let read = |p: &Path| fs::read_to_string(p).unwrap_or_default();
    let edit = |text: &str, args: &[&str]| -> String {
        sandbox.write_machine(text);
        let _ = job(&sandbox, &pool.fg, args);
        read(&path)
    };
    let edit_repo = |text: &str, args: &[&str]| -> String {
        fs::write(&config, text).expect("setup: write wtpool.toml");
        let _ = job(&sandbox, &pool.fg, args);
        read(&config)
    };
    let without_limit = "# Settings of this machine, by hand.\n[jobs]\nenforce = \"warn\"  # flag\nheavy = [\"make\"]\n";
    let without_heavy = "[jobs]\n# just a limit\nlimit = 2\n";
    let repo = format!(
        "{original}# our patterns\nheavy = [\"cargo build\"]  # ours\nbuild_processes = [\"protoc\"]\n"
    );
    let repo_prefix = format!("{original}# our patterns\nheavy = ");
    let repo_suffix = "  # ours\nbuild_processes = [\"protoc\"]\n";
    let heavy_prefix = MACHINE
        .split_once("heavy = ")
        .map(|(before, _)| format!("{before}heavy = "))
        .unwrap_or_default();

    // When
    let limit = edit(MACHINE, &["limit", "6"]);
    let enforce = edit(MACHINE, &["enforce", "require"]);
    let heavy = edit(MACHINE, &["heavy", "add", "--", "just"]);
    let reset = edit(MACHINE, &["limit", "default"]);
    let added_key = edit(without_limit, &["limit", "6"]);
    let added_heavy = edit(without_heavy, &["heavy", "add", "--", "make"]);
    let repo_added = edit_repo(
        &repo,
        &["heavy", "add", "--repo", "--", "cargo xtask verify"],
    );
    let repo_removed = edit_repo(&repo, &["heavy", "rm", "--repo", "--", "cargo build"]);
    let repo_new_key = edit_repo(&original, &["heavy", "add", "--repo", "--", "make"]);

    // Then
    let mut wrong = Vec::new();
    let mut expect = |what: &str, ok: bool, found: &str| {
        if !ok {
            wrong.push(format!("{what}: the file reads {found:?}"));
        }
    };
    expect(
        "limit 6",
        limit == MACHINE.replace("limit = 3", "limit = 6"),
        &limit,
    );
    expect(
        "enforce require",
        enforce == MACHINE.replace("enforce = \"warn\"", "enforce = \"require\""),
        &enforce,
    );
    expect(
        "heavy add -- just",
        array_between(&heavy, &heavy_prefix, "   # the build\n", &["make", "just"]),
        &heavy,
    );
    expect(
        "limit default",
        reset == MACHINE.replace("limit = 3   # three\n", ""),
        &reset,
    );
    expect(
        "limit 6 added to [jobs]",
        added_key == format!("{without_limit}limit = 6\n"),
        &added_key,
    );
    expect(
        "heavy add -- make without heavy",
        array_between(
            &added_heavy,
            &format!("{without_heavy}heavy = "),
            "\n",
            &["make"],
        ),
        &added_heavy,
    );
    expect(
        "heavy add --repo",
        array_between(
            &repo_added,
            &repo_prefix,
            repo_suffix,
            &["cargo build", "cargo xtask verify"],
        ),
        &repo_added,
    );
    expect(
        "heavy rm --repo",
        array_between(&repo_removed, &repo_prefix, repo_suffix, &[]),
        &repo_removed,
    );
    expect(
        "heavy add --repo without heavy",
        array_between(
            &repo_new_key,
            &format!("{original}heavy = "),
            "\n",
            &["make"],
        ),
        &repo_new_key,
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
