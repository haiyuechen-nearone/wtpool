# Development

[README](../README.md) · [Commands](commands.md) · [Configuration](configuration.md) ·
[Status and hook](status-and-hook.md) · [Panel](panel.md) · [Heavy jobs](heavy-jobs.md) ·
**Development**

[CONTRIBUTING.md](../CONTRIBUTING.md) has the rules for a change: the oracle files, the file size
limit and the AI policy. This page has the commands.

## Build and test

```sh
cargo build
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Run cargo directly: the repository needs no dev shell. `.cargo/config.toml` hides your global and
system git config from every cargo command, `cargo run` included, because the tests run git and
build temporary pools.

**On your own repository, run `target/debug/wtpool`**, or `target/release/wtpool`, not
`cargo run`. Under cargo, wtpool's git would run without your git config. `wtpool hook install`
refuses a binary under `target/`, because cargo replaces it on the next build. To try the hooks
with your build, install it with `cargo install --locked --path crates/wtpool`.

The extension's own build steps are in [vscode/README.md](../vscode/README.md#build).

## `cargo xtask verify`

The harness runs the checks that decide whether a build meets the spec. `cargo xtask verify`
without arguments prints its usage.

```sh
cargo xtask verify --phase 3 --part quick                           # one part
cargo xtask verify --phase 3 --part int,bench,quick,cc,ui --run mine  # every part, in waves
cargo xtask verify --phase 3 --report --run mine                    # the verdict and the report
```

- `--phase N` runs the checks of phase N and every earlier phase. Phase 3 is the last.
- `--part` names a part, or several joined by commas. Several parts first make the shared builds
  once: the release `wtpool`, the web bundle and the `.vsix`. Then they run in waves: `int` alone,
  then `bench` alone, then `quick` and `cc` together, then `ui`. A list runs its parts whole, so
  it refuses `--only`.
- `--run NAME` names the run, whose folder is `verify/out/NAME/`. Without it, the name is the first
  12 characters of the commit, plus `-dirty-` and a hash when the tree has changes. Give every part
  of one run the same name, so that `--report` finds them all.
- `--only CHECK` runs one check of a part, as in
  `--part int --only watch__should_print_alive_lines`. Use it to rerun a check you fixed.
- `--fresh` runs a part even when an earlier passing result had the same inputs. Without it, a
  part whose inputs did not change since a passing run is not run again.
- `--contract` runs the live Claude Code sessions of `cc`, and `--record` with it keeps their raw
  recordings. Both need `cc` among the parts.
- `--report` writes `verify/out/<run>/report.md` and `report.html` and prints the verdict. It
  exits 0 for PASS, 1 for FAIL and 2 for NEEDS YOU. **The verdict stays FAIL until every part has
  run.** Phase 3 needs `quick`, `int`, `ui`, `bench` and `cc`, and the report lists each missing
  part as a `part:<name>` check that was not run. The one exception is `cc` on a machine without
  recordings, below.

## What each part costs

The times are from a full phase 3 run on an Apple silicon Mac with warm build caches, about 15
minutes in all. A first run in a fresh clone also builds everything, and takes longer.

| Part | What it runs | Time | What it needs |
|---|---|---|---|
| `quick` | `static` (fmt, clippy, ESLint, stylelint, the repository's own lints and the doc link check), `render` (Vitest) and `host` (the extension's tests in VS Code's extension host) | about 4 minutes | Node and npm; VS Code. It opens VS Code windows, behind yours. |
| `int` | `unit` and the integration checks on the `verify` build, through nextest, and the replays of recorded Claude Code sessions | about 3 minutes | cargo-nextest; a quiet machine, since its watch checks time file events |
| `ui` | `shot` (Playwright screenshots in four themes, with axe) and `e2e` (one check per user flow in a real VS Code, with the `.vsix` and the release `wtpool`) | about 5 minutes | VS Code windows, which open behind yours and never take focus; Playwright's Chromium |
| `bench` | the budgets B1 to B11 | about 3 minutes | a quiet machine. It measures timings and runs alone. |
| `cc` | the Claude Code contract, judged by the recorded live result or by `int`'s replays | seconds | nothing more |
| `cc --contract` | about 12 live Claude Code sessions | about 10 minutes | `claude`, logged in, and paid model calls to `claude-haiku-5-5` |
| `--report` | coverage, `trace_complete`, `isolation__left_no_trace` and the report | under a minute | cargo-llvm-cov |

Only the live contract run costs money. Run it only when the Claude Code version changes: the
harness then has no live result for it.

## What the harness needs

- [cargo-nextest](https://nexte.st), and [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov)
  with the `llvm-tools` component (`rustup component add llvm-tools-preview`).
- Node and npm, and VS Code (see `WTPOOL_VERIFY_VSCODE` below).
- Only for the live contract run (`--contract`): `claude`, logged in, with access to the model
  `claude-haiku-5-5`.

The harness installs Playwright's Chromium on first use, which needs a network connection; later
runs reuse it. Without an installed VS Code it downloads the pinned version too.

Without a recordings folder (`$XDG_DATA_HOME/wtpool-verify/recordings/`), that is, on a machine
that never ran the live contract with `--record`, the `cc` checks count by this run's replays of
the committed fixtures alone (`cc_replay:<name>` from `int`), with or without the `cc` part, and
the report says so.

Keep `$XDG_CACHE_HOME`, by default `~/.cache`, on a short path. The `host` and `e2e` layers put
VS Code's sockets under `wtpool-verify/s/` there, and macOS caps a socket path at 103 bytes. With
a long path, the host checks fail together; `verify/out/<run>/logs/host.log` names the socket.

The harness uses no OS-specific tools but `lsappinfo` on macOS. It is tested on macOS. There
every VS Code it starts opens its windows behind yours and never takes focus, and the `host` and
`e2e` layers fail if one becomes the frontmost app; `verify/out/<run>/<layer>/front.txt` lists the
frontmost apps a layer saw.

### Module map of `vscode/test/`

Shared helpers in `support/`:

- `grammar.mjs`: the words and rows the panel should show.
- `docs.mjs`: status documents, each checked against the phase 4 schema.
- `visual-docs.mjs`: documents with long values and many chips for the visual checks.
- `jobs.mjs`: heavy job documents and the text their UI should show.
- `jobs-render.mjs`: the render layer's reach into the heavy jobs UI.
- `machine-config.mjs`: writes a sandbox's `machine.toml` by hand.
- `measure.mjs`: in-page layout and colour measurements.
- `scroll.mjs`: in-page measurements of the tab's sideways scroll.
- `pixels.mjs`: reads regions of screenshots in a blank Chromium page.
- `states.mjs`: puts the panel into the states the visual checks measure.
- `jobs-shot.mjs`: in-page measurements of the heavy jobs UI.
- `v8-lcov.mjs`: turns V8 coverage of the render and host layers into lcov.
- `code.mjs`: the launch flags of every VS Code run, with secret storage off the keychain.
- `vscode-app.mjs`: finds VS Code, from the environment, the default install or a pinned download.
- `paths.mjs`: the repository, run and fixture paths.
- `background.mjs`: keeps each VS Code run from taking focus on macOS.
- `main-hooks.mjs`: hooks VS Code's main process so it never activates and the clipboard stays in memory.
- `page.mjs`: loads the webview page in Chromium as VS Code shows it.
- `webview.mjs`: loads the webview in jsdom with a fake clock.

Helpers of the e2e layer in `e2e/`:

- `vscode.mjs`: drives a real VS Code with the packaged extension and the release `wtpool`.
- `flows.mjs`: shared steps of the flow checks, each in its own VS Code.
- `cli.mjs`: the release `wtpool` as a command line, for checks that need no window.
- `rows.mjs`: reads the panel's rows and compares them with the grammar.
- `folders.mjs`: builds the repository pools the checks open, and reads text and layout in the webview.
- `jobs-config.mjs`: writes the TOML of the heavy job flows by hand.
- `jobs-process.mjs`: stand-in commands and stub agents that wait until released.
- `jobs-dom.mjs`: DOM probes of the heavy job flows.
- `jobs.mjs`: re-exports the three `jobs-*` files.
- `agents.mjs`: the `claude` stubs of a scenario.
- `claude-stub.mjs`: one stub `claude` that runs `wtpool hook` for each request.
- `sockets.mjs`: lists the listening TCP and bound UDP sockets of some processes.
- `wtpool-old.mjs`: a wtpool from before `wtpool job`, for the checks of the old CLI.
- `global-setup.mjs`: finds VS Code once per run, downloading the pinned version if none is installed.

Spec files, by layer:

- `render/`: Vitest `*.test.mjs` checks in jsdom, one file per panel part.
- `shot/`: Playwright `*.spec.mjs` checks of the panel's pixels, one file per area: focus, jobs, layout, lines, look, output, overlap, panel, run time and scroll.
- `host/`: `*.test.cjs` checks in the extension host, run by `run.mjs` with `index.cjs`, `support.cjs` and `outputs.cjs`.
- `e2e/`: Playwright `*.spec.mjs` checks in a real VS Code: flows, panel, headings, CLI and scenarios.
- `cc/`: `run.mjs` runs the sessions of `sessions.mjs` on sandbox pools, with `sandbox.mjs` and the `recorder.mjs` hook.
- `lint/`: `rules.mjs` holds the rule groups, and `check.mjs` runs one group over the sources.
- `bench/`: `heap.spec.mjs`, the heap check.
- `fake-wtpool.mjs`, at the top: the fake `wtpool` of the host layer, which replays documents.

## npm advisories

`npm ci` in `vscode/` reports 2 high severity advisories. Both are in `@azure/msal-node`, which
`@vscode/vsce` pulls in through `@azure/identity` to publish to the Marketplace. `vsce` is a
development dependency that only packages the `.vsix` here, and neither package ships in the
extension: `.vscodeignore` keeps only the manifest, the README, the licences, the bundles and
`media/`. npm 11 also warns that the install scripts of `esbuild` and `@vscode/vsce-sign` are not
covered by `allowScripts`. The build and the package work without them.

## Environment variables

The harness reads these variables. All are optional.

| Variable | What it sets | When unset |
|---|---|---|
| `WTPOOL_VERIFY_BASE` | The commit that `cover` (changed lines) and `repo_context__should_be_unchanged` measure from. Any ref or commit. | The tag `spec-base`, else the merge base of `HEAD` with origin's default branch, else the root commit. The report names the one it used. |
| `WTPOOL_BENCH_SOURCE` | The repository the bench pool clones. Name a larger repository to measure on it. | This repository. |
| `WTPOOL_VERIFY_VSCODE` | The VS Code executable the host and end to end tests start. | The platform's default install location, else a download of the pinned version. |
| `WTPOOL_VERIFY_VSCODE_CLI` | The `code` command that installs the `.vsix` and reports the version. | The platform's default install location, else a download of the pinned version. |
| `WTPOOL_VERIFY_WRAPPER` | A command prefix for heavy jobs, such as a queue that runs one at a time. Only the parts of a comma list (`--part int,quick`) run under it; a single `--part`, and the shared builds of a list, never do. | The parts run directly. |
| `WTPOOL_VERIFY_PRIORITY` | `normal` starts every job at the normal priority. | Every job of a part but `bench`, and the shared builds, yields the processor to your work (`taskpolicy -c utility` on macOS, `nice -n 10` elsewhere); the int checks run at the normal priority once built. |
| `WTPOOL_VERIFY_WINDOWS` | `0` records the checks of `host` and `e2e`, which open VS Code windows, as not run. | The windows open. |
| `WTPOOL_VERIFY_VISIBLE_WINDOWS` | `1` keeps the VS Code windows of a run opaque, behind your windows, so you can watch it. | They are transparent and let the mouse through. |
| `WTPOOL_VERIFY_RETRIES`, `WTPOOL_VERIFY_HOST_RETRIES` | `0` runs each check of `shot`, `e2e` and `host` once, as freeze does. Any other value stops the run. | The runners retry a failed check twice, and a pass on a retry counts as flaky. |
| `WTPOOL_VERIFY_HOST_ORDER`, `WTPOOL_VERIFY_HOST_INVERT` | For a host run by hand (`vscode/test/host/run.mjs`): `reverse` runs the checks in reverse order, `1` runs every check but those `WTPOOL_VERIFY_HOST_GREP` selects. A verify run takes only `0` and stops on any other value. | The checks run in file order. |
| `WTPOOL_VERIFY_E2E_WORKERS` | How many e2e checks run at once. | 4. |
| `XDG_CACHE_HOME` | Where `wtpool-verify/` keeps clean clones and other caches. | `~/.cache`, so `~/.cache/wtpool-verify`. |
| `XDG_DATA_HOME` | Where `wtpool-verify/` keeps the build copy of `wtpool` for the contract and the recordings. | `~/.local/share`, so `~/.local/share/wtpool-verify`. |
| `WTPOOL_FOREGROUND` | Your own pool, which `isolation__left_no_trace` checks for traces of the run. | The checks on your own pool are skipped. |

`WTPOOL_VERIFY_BASE` is a variable, not a flag:

```sh
WTPOOL_VERIFY_BASE=main cargo xtask verify --phase 3 --report
```

[Privacy and uninstall](privacy-and-uninstall.md#for-contributors) says what the two
`wtpool-verify/` folders hold and how to delete them.

### Variables the harness sets

The harness passes these to its own processes. Do not set them yourself; this table is for reading
the code.

| Variable | Set by | For | What it holds |
|---|---|---|---|
| `WTPOOL_VERIFY` | `host/run.mjs` | the extension | `1`: `activate` returns the test interface and the clock is fake. |
| `WTPOOL_VERIFY_SANDBOX` | every job (`Ctx::command`), the host, e2e and cc layers | `wtpool hook` | `1`: the hook runs in a sandbox pool. |
| `WTPOOL_VERIFY_CLOCK`, `WTPOOL_VERIFY_PROCS` | the int checks, `cc_replay` | the `verify` build of `wtpool` | Files with the fake time and the fake process table. |
| `WTPOOL_VERIFY_PANIC` | the int checks | the `verify` build of `wtpool hook` | The event name on which the hook panics. |
| `WTPOOL_VERIFY_RUN_DIR` | every layer | `xtask/tests`, `vscode/test/support/paths.mjs` | The run folder, below which each layer writes. |
| `WTPOOL_VERIFY_REPO` | the int and bench parts | `xtask/tests`, the bench layer | The tree under test. |
| `WTPOOL_VERIFY_PACKAGE`, `WTPOOL_VERIFY_BUILD_DIRTY`, `WTPOOL_VERIFY_REBUILD_STEPS` | the int part | `xtask/tests` | The package with the `wtpool` binary, `1` for a dirty tree, and the rebuild steps of the version check. |
| `WTPOOL_VERIFY_CORES` | the int part | the `verify` build of `wtpool` | The core count that the default job limit is a quarter of, in place of the system's count. |
| `WTPOOL_VERIFY_XTASK`, `WTPOOL_VERIFY_RELEASE_BIN`, `WTPOOL_VERIFY_PHASE` | the e2e, cc and bench layers | `e2e/`, `cc/`, `bench/` | The xtask binary that builds pools, the release `wtpool`, and the phase. |
| `WTPOOL_VERIFY_HOST_COVERAGE`, `WTPOOL_VERIFY_HOST_GREP` | the host layer | `host/run.mjs` | The V8 coverage folder, and the titles an `--only` run selects. |
| `WTPOOL_VERIFY_HOST_NAME` | a host run by hand | `host/run.mjs` | The layer folder below the run folder, `host` by default. |
| `WTPOOL_VERIFY_RESULTS`, `WTPOOL_VERIFY_HOST_DIR`, `WTPOOL_VERIFY_HOST_FAKES`, `WTPOOL_VERIFY_WORKSPACE_FAKE`, `WTPOOL_VERIFY_OTHER_FOLDER` | `host/run.mjs` | the host checks | The results file, the layer folder, the fake `wtpool` folder, the workspace's fake and a second folder. |
| `WTPOOL_VERIFY_CC_WTPOOL`, `WTPOOL_VERIFY_CC_RECORDINGS`, `WTPOOL_VERIFY_CC_RESULTS`, `WTPOOL_VERIFY_CC_CHECKS` | the cc layer | `cc/run.mjs` | The copy of `wtpool`, the recordings folder, the results file and the wanted checks. |
| `WTPOOL_VERIFY_CC_RECORDING`, `WTPOOL_VERIFY_CC_PARENTS` | `cc/run.mjs` | `cc/recorder.mjs` | The files a live session's hook payloads and parent processes go to. |

## Add a check

Every file below is an oracle file, so the change needs an entry in `verify/oracle-changes.toml`
and an accept line ([CONTRIBUTING.md](../CONTRIBUTING.md)). Name a check `<sut>__should_<assertion>`.

1. Write the check where its part runs it:
   - an int check: a test in `xtask/tests/checks/`;
   - a check of a JavaScript layer: a test in `vscode/test/<layer>/`;
   - a check the xtask computes: a function that returns a `CheckResult`, called from its part.
2. Tell the harness where it runs:
   - a check of a layer but `int` goes in `verify/checks.toml`, with its layers. A layer fails a
     listed check it reported no result for, and an unlisted one it reported;
   - an int check needs nothing: `part_of` (`xtask/src/clone/inventory.rs`) sends every name it does
     not know to `int`;
   - a harness check, with no ID, goes in `HARNESS_CHECKS` of that file, and in `part_of` when its
     part is not `int`.
3. Trace it in `verify/trace.toml`: in `checks` of each ID it checks, or for a harness check in
   `[harness]` with its phase, and in the same place of verification.md, whose trace list the file
   copies. A check that starts later than its ID's phase also goes in `[check_phase]`.
4. Run it once with `--only` and see it fail on the old behaviour. At the next freeze each new check
   must fail on an assertion against the frozen tree: give it a patch in `verify/canaries/` when it
   cannot, or list it in `verify/absent.toml` when it tests an absence.

The tool checks of the quick part, `static:*` for the product and `harness:*` for `xtask/`
(`xtask/src/parts/quick.rs`), go in none of these files: their prefix sends them to `quick`.

## Try a build on a scratch pool

Set `WTPOOL_FOREGROUND`, `WTPOOL_SLOT_DIR` and `HOME` to temporary folders, never your real pool:

```sh
git init -q /tmp/wtpool-sandbox/repo && git -C /tmp/wtpool-sandbox/repo commit -q --allow-empty -m start
HOME=/tmp/wtpool-sandbox WTPOOL_FOREGROUND=/tmp/wtpool-sandbox/repo WTPOOL_SLOT_DIR=/tmp/wtpool-sandbox \
  target/release/wtpool setup
```

## Recording the demo GIF

`docs/images/demo.gif`, the animation of the Worktrees tab, is recorded by a script, so you can
make it again after the tab changes. You need VS Code (found as the harness finds it, see
`WTPOOL_VERIFY_VSCODE` above) and [gifski](https://gif.ski), the GIF encoder:

```sh
cargo install --locked gifski
cd vscode && npm ci && (cd test && npm ci)
npm run package            # wtpool.vsix, the extension the recording installs
node scripts/demo.mjs      # records the story and writes docs/images/demo.gif
```

Without gifski the script stops and says how to install it. It opens a VS Code window for about
half a minute, behind all your windows on macOS, and it never takes focus; leave that window
alone while it records.

It runs in a sandbox below the system temporary folder: HOME, VS Code's user data and extensions,
and the workspace folder are all new, and `wtpool` is the harness's fake (`vscode/test/fake-wtpool.mjs`)
printing scripted documents. Your pool, settings and extensions are never used, and the sandbox
is deleted at the end (`--keep` leaves it).

- **The story** is in `vscode/scripts/demo/story.mjs`: the placeholders (`acme/app`, `my-mac`,
  the paths), each status document, the test log and the pointer's moves, on one timeline of
  18 seconds. Every time a document holds comes from that timeline, and the webview's clock is
  shifted to it, so ages and clock times are the same on every run. The detail strip with its
  Output section is closed again before the pointer goes to the last row, so every step fits the
  600 px window.
- **Later features.** Three parts of the story follow features that may be newer than your
  checkout: a session's title as its label (H16), a background task's live output in the detail
  strip, and an agent whose turn ended working on while its background task runs (A1). The script
  reads from the spec and the generated schema whether this checkout has them, and records the
  story as this version shows it. `--features all`, `none` or a list such as
  `labels,outputs,background` overrides that.
- **The frames.** The window is 1000 × 600 in Dark Modern. Screenshots are taken at twice that
  size about 14 times a second, and gifski scales them to 1000 px wide at 10 frames a second.
  The pointer and the click ring are drawn into the window, because screenshots show no cursor.
  `--width`, `--fps` and `--quality` tune the GIF; `--frames <dir>` keeps the screenshots in a
  new or empty folder, and refuses one that holds files; `--keyframes <dir>` writes one PNG per
  step.

## Where things live

- Code: `crates/wtpool-core` holds the pool, `wtpool status`, the activity records and the rules
  of `wtpool hook`, the same for every coding agent harness and git hosting service;
  `crates/wtpool-claude-code` is the Claude Code adapter, which reads its hook payloads,
  transcripts and `settings.json`; `crates/wtpool-github` is the GitHub adapter, which reads PR
  refs and remote URLs and looks PR heads up with `gh`; and `crates/wtpool` is the binary, which
  picks the adapters. The extension is in `vscode/src`.
- Spec: [docs/design/status-panel/status-panel.md](design/status-panel/status-panel.md).
- How each behaviour ID is checked: [verification.md](design/status-panel/verification.md).
- Reviews and live results: `verify/reviews/`.
- How Claude Code agents specified, tested and built wtpool, and what lives where in `verify/`:
  [how-it-was-built.md](how-it-was-built.md).
- Approved screenshots, the test baselines: `verify/shots/approved/`. The PNGs in
  `docs/images/` are cropped copies of them; the two `how-it-works` SVGs are drawn by hand, and
  `demo.gif` is recorded by `node scripts/demo.mjs`.
