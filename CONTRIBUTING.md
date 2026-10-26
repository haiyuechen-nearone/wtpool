# Contributing

Bug reports, fixes and ideas are welcome. For a large change, open an issue first, so that the
behaviour is agreed before you write code. Report security problems privately, as
[SECURITY.md](SECURITY.md) says.

## Build and test

You need git, Rust 1.95 or newer, and for the extension Node and npm.

```sh
cargo build
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
cd vscode && npm ci && npm run lint && npm run build
```

Run cargo directly, with no dev shell. `.cargo/config.toml` hides your git config from cargo
commands, so try your build on a repository with `target/debug/wtpool`, not `cargo run`.
[Development](docs/development.md) has the details and a scratch pool to try a build on.

## The verify harness

`cargo xtask verify` runs the checks that decide whether a build meets the spec,
[status-panel.md](docs/design/status-panel/status-panel.md). It has five parts. Run the ones your
change touches, then all of them and `--report` before you ask for a merge, if you can.

| Part | Time | What it needs |
|---|---|---|
| `quick` | about 4 minutes | Node, npm, VS Code. It opens VS Code windows. |
| `int` | about 3 minutes | cargo-nextest and a quiet machine |
| `ui` | about 5 minutes | VS Code windows, which open behind yours, Playwright's Chromium (downloaded once) |
| `bench` | about 3 minutes | a quiet machine; it measures timings |
| `cc` | seconds | nothing more: it uses the recorded live result or the replays |
| `cc --contract` | about 10 minutes | a Claude Code login, and paid model calls to `claude-haiku-5-5` |

The times are from an Apple silicon Mac with warm caches. `--report` gives PASS only once every
part has run. You never need `--contract` for a change: it is for a new Claude Code version.
[Development](docs/development.md#cargo-xtask-verify) explains every flag.

If you cannot run a part, for example `ui` without a desktop, say so in the pull request.

## Oracle files

The checks and what they read are the oracle. They decide PASS, so a change to them needs a
reason that someone else can review. The oracle files are `docs/design/`, `xtask/`,
`vscode/test/`, `verify/` except `out/`, `reviews/`, `accepted.txt` and `oracle-changes.toml`, and
every file that `verify/oracle.lock` lists, such as `clippy.toml` and `vscode/package.json`.

A change to an oracle file needs both of these in the same pull request:

1. One `[[change]]` entry at the end of `verify/oracle-changes.toml`, with a new `id`, a `reason`
   in neutral words, and a `files` table with each path it changes, or `file:key` for an oracle
   key such as `Cargo.toml:workspace.lints`, and its SHA 256 after the change, or `"missing"` for
   a path it deletes:

   ```toml
   [[change]]
   id = "my-change"
   reason = "What the check judged before, what it judges now, and why the old check was wrong."
   [change.files]
   "xtask/tests/checks/watch_changes.rs" = "<sha256 of the file after the change>"
   ```

   You need not compute the hashes: the failure message of
   `cargo xtask verify --phase 3 --part quick --only oracle_unchanged` ends with the
   `[change.files]` lines for every path that no entry pins yet.

2. A line `accept <id> <entry hash>` in `verify/accepted.txt`, after a `#` comment that says what
   it accepts. The entry hash covers the entry's whole content, and the `oracle_change` message
   prints the line. A maintainer adds or confirms that line when they agree with the change.

Without an entry that pins the current hash of each changed path, the check `oracle_unchanged`
fails. A later edit of the same file changes its hash, so it needs a new entry. With an entry but
no `accept` line for its current content, the report says NEEDS YOU (`oracle_change`), so an entry
that is widened after it was accepted needs a new `accept` line. Any other line in
`verify/accepted.txt`, apart from `#` comments, fails the check. Each `cargo xtask freeze` records
the ids of all entries in `verify/oracle.lock`, after which they allow no change again. Why: whoever
can quietly weaken a check can make any build pass. The entry makes each change to a check visible,
with its reason, and the `accept` line records that a person agreed with it.
[How it was built](docs/how-it-was-built.md#the-loop-oracle-first) tells the story.

Check that the file still parses:

```sh
python3 -c "import tomllib;tomllib.load(open('verify/oracle-changes.toml','rb'))"
```

A change to behaviour starts in the spec: edit the row of its ID in
[status-panel.md](docs/design/status-panel/status-panel.md), with a revision note, in the same
pull request.

## Code style

- A source file stays around 500 lines, and never goes over 1000. Split a longer one into
  modules.
- Match the code around your change in naming, comments and idiom. Comments cite spec IDs, such
  as `(H6)`, as the code already does.
- Rust test names follow `<sut>__should_<assertion>`, with `// Given`, `// When` and `// Then`.
  The full test standards are in [verification.md](docs/design/status-panel/verification.md#test-standards).
- Open an issue before you add a crate or an npm package. The product's dependencies are pinned
  to exact versions; keep them so.
- Docs use short sentences in plain English, and the placeholders `acme/app`, `~/code/app`,
  `app-wt1` and `my-mac`.

## Use of AI

This repository was itself built by coding agents, as
[How it was built](docs/how-it-was-built.md) describes. Contributions made with AI tools are
welcome. A person must review every change and stand behind it: you have read each line, you can
explain it, and you ran the tests you say you ran. Say in the pull request which tools you used.
Do not let an agent open pull requests or answer reviews in your name without reading them.

## Licence

wtpool is MIT licensed ([LICENSE](LICENSE)). Inbound equals outbound: by contributing, you agree
that your contribution is licensed under the same MIT licence. Add only code you have the right
to license that way. A file copied from elsewhere keeps its own licence, names its source in a
header where its format allows one, and gets an entry in
[THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
