use std::io;
use std::path::PathBuf;

use rstest::rstest;

use crate::names::{InvalidSlotName, PrNumber};

use super::*;

const HEAD: &str = "1111111111111111111111111111111111111111";

fn path(path: &str) -> PathBuf {
    PathBuf::from(path)
}

fn io_error(text: &str) -> io::Error {
    io::Error::other(text.to_owned())
}

fn dirty(worktree: &str) -> Refusal {
    Refusal::WorktreeDirty {
        worktree: worktree.to_owned(),
        count: 1,
    }
}

fn on_new_slot(slot: &str, code: Option<i32>) -> Error {
    Error::OnNewSlot {
        command: "direnv allow".to_owned(),
        slot: path(slot),
        code,
    }
}

fn command_failed(code: Option<i32>) -> Error {
    GitError::CommandFailed {
        program: "git".to_owned(),
        code,
    }
    .into()
}

#[rstest]
#[case::usage(Error::Usage("usage: wtpool ls"), &["usage: wtpool ls"], 1)]
#[case::unknown_flag(Error::UnknownFlag("--x".to_owned()), &["unknown flag '--x'"], 1)]
#[case::invalid_ref(
    Error::InvalidRef("a..b".to_owned()),
    &["'a..b' is not a PR number, a github.com PR URL or a valid branch name"],
    1
)]
#[case::invalid_slot_name(
    Error::InvalidSlotName { name: "-x".to_owned(), reason: InvalidSlotName::LeadingDash },
    &["invalid slot name '-x': a slot name cannot start with '-'"],
    1
)]
#[case::slot_name_without_the_prefix(
    Error::InvalidSlotName {
        name: "x".to_owned(),
        reason: InvalidSlotName::MissingPrefix("WTPOOL_SLOT_PREFIX 'app-wt'".to_owned()),
    },
    &["invalid slot name 'x': a slot name must start with WTPOOL_SLOT_PREFIX 'app-wt'"],
    1
)]
#[case::no_match(Error::NoMatch("x".to_owned()), &["no worktree matches 'x'"], 1)]
#[case::not_checked_out(
    Error::NotCheckedOut("a".to_owned()),
    &["a is not checked out in any worktree"],
    1
)]
#[case::no_such_branch_on_the_remote(
    Error::NoSuchBranch { branch: "a".to_owned(), remote: Some("origin".to_owned()) },
    &["no local branch and no origin branch named 'a'"],
    1
)]
#[case::no_such_branch_without_a_remote(
    Error::NoSuchBranch { branch: "a".to_owned(), remote: None },
    &["no branch 'a' here, and no remote to look it up"],
    1
)]
#[case::unresolved_without_a_cause(
    Error::NotResolved { reference: "42".to_owned(), consequence: "no slot changed", cause: None },
    &["'42': no slot changed"],
    1
)]
#[case::unresolved_cause_first(
    Error::NotResolved {
        reference: "a".to_owned(),
        consequence: "foreground not changed",
        cause: Some(Box::new(Error::NoSuchBranch {
            branch: "a".to_owned(),
            remote: Some("origin".to_owned()),
        })),
    },
    &["no local branch and no origin branch named 'a'", "'a': foreground not changed"],
    1
)]
#[case::unresolved_after_a_command_that_printed_its_own_error(
    Error::NotResolved {
        reference: "a".to_owned(),
        consequence: "no slot changed",
        cause: Some(Box::new(command_failed(Some(128)))),
    },
    &["'a': no slot changed"],
    1
)]
#[case::unresolved_after_a_command_that_could_not_run(
    Error::NotResolved {
        reference: "a".to_owned(),
        consequence: "no slot changed",
        cause: Some(Box::new(GitError::Spawn { program: "gh".to_owned(), source: io_error("denied") }.into())),
    },
    &["could not run gh: denied", "'a': no slot changed"],
    1
)]
#[case::lock(
    Error::Lock { path: path("/r/.git/wtpool.lock"), source: io_error("denied") },
    &["could not lock /r/.git/wtpool.lock: denied"],
    1
)]
#[case::watch(
    Error::Watch { path: path("/r/.git"), reason: "gone".to_owned() },
    &["could not watch /r/.git: gone"],
    1
)]
#[case::slot_setup_after_each_failure(
    Error::SlotSetup {
        failures: vec![
            on_new_slot("/w/app-wt1", Some(127)),
            on_new_slot("/w/app-wt2", None),
            Error::Copy { path: ".env".to_owned(), slot: path("/w/app-wt3"), source: io_error("disk full") },
            GitError::IgnoreUnknown { path: ".env".to_owned(), main: path("/w/app") }.into(),
            Error::NoMainWorktree(path("/w/app-wt4")),
            GitError::Spawn { program: "sh".to_owned(), source: io_error("not found") }.into(),
            command_failed(Some(2)),
        ],
    },
    &[
        "on_new_slot 'direnv allow' failed in /w/app-wt1: exit status 127",
        "on_new_slot 'direnv allow' failed in /w/app-wt2: killed by a signal",
        "could not copy .env into /w/app-wt3: disk full",
        "could not tell whether git ignores .env in /w/app; not copied",
        "nothing copied into /w/app-wt4: the repository has no main worktree to copy from",
        "could not run sh: not found",
        "copy or on_new_slot failed as shown above; the slots were kept",
    ],
    1
)]
#[case::foreground_dirty(
    Refusal::ForegroundDirty { count: 2, reference: "feature".to_owned() }.into(),
    &["foreground has 2 uncommitted file(s); commit, stash, or use: wtpool bg feature"],
    1
)]
#[case::holder_dirty(
    Refusal::HolderDirty { branch: "feature".to_owned(), holder: "app-wt1".to_owned(), count: 1 }.into(),
    &["feature is checked out in app-wt1 with 1 uncommitted file(s); commit or stash them there first"],
    1
)]
#[case::held_by_foreground(
    Refusal::HeldByForeground { branch: "feature".to_owned(), path: path("/w/app") }.into(),
    &["feature is checked out in the foreground (/w/app); work on it there, or first move the foreground to another branch with 'wtpool fg <other-branch>'"],
    1
)]
#[case::held_by_unmanaged(
    Refusal::HeldByUnmanaged {
        branch: "feature".to_owned(),
        worktree: "review".to_owned(),
        path: path("/w/review"),
    }.into(),
    &["feature is checked out in the unmanaged worktree review (/w/review), which is not a slot; work on it there, or first release it with 'wtpool free feature'"],
    1
)]
#[case::full_pool_after_each_blocked_slot(
    Refusal::PoolFull {
        limit: 3,
        blocked: vec![
            Refusal::SlotHoldsBranch { slot: "app-wt1".to_owned(), branch: "a".to_owned() },
            dirty("app-wt2"),
            Refusal::HeadUnknown("app-wt3".to_owned()),
        ],
    }.into(),
    &[
        "app-wt1 holds branch a",
        "app-wt2 has 1 uncommitted file(s)",
        "could not tell what app-wt3 has checked out; not reusing it",
        "no free slot and the pool is at its 3 slot limit; release one with 'wtpool free <slot>' (see 'wtpool ls')",
    ],
    1
)]
#[case::detach_foreground(
    Refusal::DetachForeground.into(),
    &["refusing to detach the foreground worktree"],
    1
)]
#[case::remove_foreground(
    Refusal::RemoveForeground.into(),
    &["refusing to remove the foreground worktree"],
    1
)]
#[case::dirty(dirty("app-wt1").into(), &["app-wt1 has 1 uncommitted file(s)"], 1)]
#[case::busy_agents_with_their_labels_as_given(
    Refusal::AgentsBusy {
        worktree: "app-wt1".to_owned(),
        agents: vec!["Fix\u{1b}[31m \u{202e}login (working)".to_owned(), "Explore (waiting)".to_owned()],
    }.into(),
    &["app-wt1 has 2 agent(s) working or waiting: Fix\u{1b}[31m \u{202e}login (working), Explore (waiting); wait until they are idle, or use --force"],
    1
)]
#[case::status_unknown(
    Refusal::StatusUnknown("app-wt1".to_owned()).into(),
    &["could not read the status of app-wt1; not touching it"],
    1
)]
#[case::unreferenced(
    Refusal::Unreferenced { worktree: "app-wt1".to_owned(), count: 2, head: HEAD.to_owned() }.into(),
    &["app-wt1 holds 2 commit(s) on no branch; to keep them, run: git branch <name> 1111111111111111111111111111111111111111"],
    1
)]
#[case::unreferenced_unknown(
    Refusal::UnreferencedUnknown("app-wt1".to_owned()).into(),
    &["could not tell whether app-wt1 holds commits on no branch; not touching it"],
    1
)]
#[case::operation_after_a_vowel(
    Refusal::OperationInProgress { worktree: "app-wt1".to_owned(), operation: "am" }.into(),
    &["app-wt1 is in the middle of an am; finish or abort it first"],
    1
)]
#[case::operation_after_a_consonant(
    Refusal::OperationInProgress { worktree: "app-wt1".to_owned(), operation: "rebase" }.into(),
    &["app-wt1 is in the middle of a rebase; finish or abort it first"],
    1
)]
#[case::operation_unknown(
    Refusal::OperationUnknown("app-wt1".to_owned()).into(),
    &["could not tell whether an operation is in progress in app-wt1; not touching it"],
    1
)]
#[case::holder_gone(
    Refusal::HolderGone {
        holder: "app-wt1".to_owned(),
        branch: "a".to_owned(),
        remove: "app-wt1".to_owned(),
    }.into(),
    &["app-wt1 has a checked out but its directory is gone; run 'wtpool rm app-wt1' or restore the directory"],
    1
)]
#[case::worktree_gone(
    Refusal::WorktreeGone { worktree: "review".to_owned(), remove: "/w/review".to_owned() }.into(),
    &["review is registered as a worktree but its directory is gone; run 'wtpool rm /w/review'"],
    1
)]
#[case::path_exists(
    Refusal::PathExists(path("/w/app-wt1")).into(),
    &["/w/app-wt1 already exists"],
    1
)]
#[case::no_commits(
    Refusal::NoCommits(path("/w/app")).into(),
    &["/w/app has no commits yet, so a slot has nothing to check out; make a first commit, then try again"],
    1
)]
#[case::invalid_variable(
    SettingsError::InvalidVariable {
        var: "WTPOOL_SLOT_COUNT",
        value: "0".to_owned(),
        reason: "must be at least 1".to_owned(),
    }.into(),
    &["invalid WTPOOL_SLOT_COUNT='0': must be at least 1"],
    1
)]
#[case::invalid_value(
    SettingsError::InvalidValue {
        key: "slot_count",
        value: "0".to_owned(),
        origin: "in /r/.git/wtpool.toml".to_owned(),
        reason: "must be at least 1".to_owned(),
    }.into(),
    &["invalid slot_count='0' in /r/.git/wtpool.toml: must be at least 1"],
    1
)]
#[case::config_syntax_on_a_line(
    SettingsError::ConfigSyntax { path: path("/r/.git/wtpool.toml"), line: Some(3), reason: "oops".to_owned() }.into(),
    &["invalid /r/.git/wtpool.toml: line 3: oops"],
    1
)]
#[case::config_syntax_on_no_line(
    SettingsError::ConfigSyntax { path: path("/r/.git/wtpool.toml"), line: None, reason: "oops".to_owned() }.into(),
    &["invalid /r/.git/wtpool.toml: oops"],
    1
)]
#[case::read_config(
    SettingsError::ReadConfig { path: path("/r/.git/wtpool.toml"), reason: "not a regular file".to_owned() }.into(),
    &["could not read /r/.git/wtpool.toml: not a regular file"],
    1
)]
#[case::config_symlink(
    SettingsError::ConfigSymlink(path("/r/.git/wtpool.toml")).into(),
    &["refusing to read /r/.git/wtpool.toml: it is a symlink"],
    1
)]
#[case::no_repository(
    SettingsError::NoRepository.into(),
    &["not inside a git repository: run wtpool in your repository or set WTPOOL_FOREGROUND"],
    1
)]
#[case::no_repository_at(
    SettingsError::NoRepositoryAt(path("/w/x")).into(),
    &["no git repository at /w/x (foreground from env WTPOOL_FOREGROUND)"],
    1
)]
#[case::repository_unknown(
    SettingsError::RepositoryUnknown(path("/w/x")).into(),
    &["could not tell which git repository /w/x belongs to, so wtpool cannot find its pool. Run wtpool from another directory, or set WTPOOL_FOREGROUND to the foreground worktree"],
    1
)]
#[case::bare_repository(
    SettingsError::BareRepository {
        repository: path("/r/app.git"),
        file: path("/r/app.git/wtpool.toml"),
        key: "foreground",
        var: "WTPOOL_FOREGROUND",
    }.into(),
    &["/r/app.git is a bare repository, so it has no main worktree to derive foreground from; set WTPOOL_FOREGROUND, or foreground in /r/app.git/wtpool.toml"],
    1
)]
#[case::foreground_missing(
    SettingsError::ForegroundMissing(path("/w/app")).into(),
    &["foreground worktree /w/app does not exist"],
    1
)]
#[case::spawn(
    GitError::Spawn { program: "git".to_owned(), source: io_error("not found") }.into(),
    &["could not run git: not found"],
    1
)]
#[case::status_of_a_failed_command(command_failed(Some(128)), &[], 128)]
#[case::failed_command_that_claims_success(command_failed(Some(0)), &[], 1)]
#[case::command_killed_by_a_signal(command_failed(None), &[], 1)]
#[case::list_worktrees(
    GitError::ListWorktrees(path("/w/app")).into(),
    &["could not list the worktrees of /w/app"],
    1
)]
#[case::no_git_dir(
    GitError::NoGitDir(path("/w/app")).into(),
    &["could not find the git directory of /w/app"],
    1
)]
#[case::add_worktree(
    GitError::AddWorktree(path("/w/app-wt1")).into(),
    &["could not create worktree at /w/app-wt1"],
    1
)]
#[case::fetch(GitError::Fetch.into(), &["fetch failed"], 1)]
#[case::create_branch(GitError::CreateBranch("a".to_owned()).into(), &["could not create branch a"], 1)]
#[case::fetch_pr(GitError::FetchPr(PrNumber::new("42").unwrap()).into(), &["could not fetch PR #42"], 1)]
#[case::fetch_branch(GitError::FetchBranch("a".to_owned()).into(), &["could not fetch branch a"], 1)]
#[case::look_up_remote(
    GitError::LookUpRemote { branch: "a".to_owned(), remote: "origin".to_owned() }.into(),
    &["could not look up branch 'a' on origin"],
    1
)]
fn error__should_print_its_lines_and_exit_with_its_code(
    #[case] error: Error,
    #[case] lines: &[&str],
    #[case] code: i32,
) {
    // When
    let (messages, exit_code) = (error.messages(), error.exit_code());

    // Then
    assert_eq!(messages, lines);
    assert_eq!(exit_code, code);
}

#[rstest]
#[case::cause_of_an_unresolved_ref(
    Error::NotResolved {
        reference: "a".to_owned(),
        consequence: "no slot changed",
        cause: Some(Box::new(GitError::Fetch.into())),
    },
    Some("fetch failed")
)]
#[case::io_error_of_a_lock(
    Error::Lock { path: path("/r/.git/wtpool.lock"), source: io_error("denied") },
    Some("denied")
)]
#[case::reason_of_an_invalid_slot_name(
    Error::InvalidSlotName { name: "-x".to_owned(), reason: InvalidSlotName::LeadingDash },
    Some("a slot name cannot start with '-'")
)]
#[case::io_error_through_a_group(
    GitError::Spawn { program: "git".to_owned(), source: io_error("not found") }.into(),
    Some("not found")
)]
#[case::refusal(Refusal::DetachForeground.into(), None)]
fn source__should_give_the_cause(#[case] error: Error, #[case] expected: Option<&str>) {
    // When
    let source = std::error::Error::source(&error).map(ToString::to_string);

    // Then
    assert_eq!(source.as_deref(), expected);
}
