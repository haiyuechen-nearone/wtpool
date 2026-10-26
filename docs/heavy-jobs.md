# Heavy jobs

[README](../README.md) · [Commands](commands.md) · [Configuration](configuration.md) ·
[Status and hook](status-and-hook.md) · [Panel](panel.md) · **Heavy jobs** ·
[Development](development.md)

Agents that work in parallel each start builds and test suites on their own. Four `cargo test`
runs at once compete for the same cores and memory: each takes longer than it would alone, tests
that measure time fail, and the machine may start to swap. wtpool queues heavy commands under one
limit per machine, so they take turns.

## Why queue heavy commands

- `wtpool job -- <command>` takes a place under the machine's limit and runs the command. When
  every place is taken, it waits in line, and waiting jobs start in the order they came.
- The command runs exactly as you gave it: the same arguments, streams, exit code and signals.
  Nothing rewrites it.
- The limit is one per user and machine, shared by every repository, every agent and your own
  terminals.
- There is no daemon and no socket. Each job holds a lock file, so a job that dies frees its place
  at once.
- One list of heavy commands drives everything: the names of jobs, the builds that `wtpool status`
  and the [panel](panel.md#heavy-jobs) show, and the policy that flags or refuses heavy commands
  run outside the queue.

## Start

1. **Accept the suggestions.** wtpool suggests heavy commands from the files at the top of your
   repository. Nothing is heavy until you accept it. In the repository, run:

   ```sh
   cd ~/code/app
   wtpool job heavy suggest                  # each suggestion, with the file it came from
   wtpool job heavy add --repo --suggested   # accept them all
   ```

   Or open the machine strip in the panel and press **Add all**. The patterns go to
   `.git/wtpool.toml`, for this repository only. Add your own the same way, or without `--repo`
   for every repository on this machine:

   ```sh
   wtpool job heavy add --repo -- 'cargo xtask verify'
   wtpool job heavy add -- just
   ```

2. **Set a limit.** The default is a quarter of the cores, kept from 1 to 8, so 16 cores give 4.
   To change it, run `wtpool job limit 6`. `wtpool job limit default` goes back to the default.

3. **Tell your agents.** For Claude Code, add this line to your `CLAUDE.md`, such as
   `~/.claude/CLAUDE.md`:

   ```
   Before builds and tests, run wtpool job ls once; run each heavy command it lists as wtpool job -- <command>.
   ```

   The plugin also gives Claude Code a skill that says the same, as
   [Claude Code and other agent tools](#claude-code-and-other-agent-tools) says.

4. **Check.** `wtpool job ls` prints the limit, the policy, the heavy commands and the form to run
   them in, then the jobs. The panel's heading reads `acme/app · my-mac · jobs 2/6`. Click the
   machine name to see and change the settings.

### What wtpool suggests

| At the top of the repository | Suggests |
|---|---|
| `Cargo.toml` | `cargo build`, `cargo test`, `cargo nextest`, `cargo clippy` |
| `go.mod` | `go build`, `go test` |
| `pom.xml` | `mvn`, or `mvnw` when `mvnw` is there |
| `build.gradle`, `build.gradle.kts` | `gradle`, or `gradlew` when `gradlew` is there |
| `pyproject.toml`, `pytest.ini` | `pytest` |
| `tox.ini` | `tox`, `pytest` |
| `BUILD.bazel`, `WORKSPACE`, `MODULE.bazel` | `bazel build`, `bazel test` |
| `Makefile`, `makefile`, `GNUmakefile` | `make` |
| `CMakeLists.txt` | `cmake`, `ninja` |
| `*.csproj`, `*.sln` | `dotnet build`, `dotnet test` |
| `Package.swift` | `swift build`, `swift test` |
| `*.xcodeproj` | `xcodebuild` |
| `package.json` | its build and test scripts, below |

wtpool looks at the main worktree, or at the foreground of a bare repository. No file suggests
`cargo check`: add it yourself to queue it. A suggestion already in a list is not shown.

From `package.json` wtpool reads only the names of `scripts`, never their commands. A script
counts when its name is `build`, `test`, `compile`, `bundle` or `e2e`, or starts with one of them
and `:`, `-` or `_`, as `test:unit` does. Each gives `<manager> run <name>`. pnpm and yarn also give
`<manager> <name>`, and npm gives `npm test` for `test`. The manager is the one `packageManager`
names (`npm`, `pnpm`, `yarn` or `bun`), else the one whose lockfile is there, else npm. At most 20
scripts count. So scripts `build`, `test`, `test:e2e` and `start` with a `pnpm-lock.yaml` give
`pnpm run build`, `pnpm build`, `pnpm run test`, `pnpm test`, `pnpm run test:e2e` and
`pnpm test:e2e`, and nothing for `start`.

Until you accept a pattern, the suggestions show running builds in `wtpool status` and the panel,
and do nothing else: they never queue, flag or refuse a command.

## `warn` and `require`

`wtpool job enforce` sets what happens to a heavy command that runs outside a job:

| Policy | What happens |
|---|---|
| `off` | Nothing is flagged. |
| `warn`, the default | A heavy process outside a job reads `build: make (not queued)` in the panel, the heading counts it as `· 1 not queued`, and `wtpool job ls` lists it under `not queued`. |
| `require` | As `warn`. Also, Claude Code refuses an agent's heavy shell command that no job wraps, before it runs, and names the wrapped command to run instead. |

Under `require`, an agent that runs `cd ~/code/app-wt1 && cargo test` through Bash gets this
refusal, and nothing runs:

```
wtpool: cargo test is heavy on this machine (enforce = require). Run it as: cd ~/code/app-wt1 && wtpool job -- cargo test
```

The agent then runs its own command in that form. Its job queues, and its line in the panel reads
`queued: cargo test` until its turn comes. The reason is the agent's own command, with
`wtpool job -- ` before each heavy part, after any `VAR=value` words. For a command over 2000
characters it lists the heavy commands instead:
`Run each heavy command as wtpool job -- <command>: cargo test, make`.

- Nothing ever rewrites a command. `require` refuses it, and the agent runs the wrapped form.
- A command inside a job is never judged: one that `wtpool job` wraps, and any command while
  `WTPOOL_JOB` names a live job, such as each step of a wrapped script.
- `require` guides agents that cooperate. It is not a security boundary. An agent runs as you,
  so it can run `wtpool job enforce off` itself. The check fails open:
  when it cannot read the call, the config or the command, or does not answer in time, it says
  nothing and the call goes on. A heavy command started another way, such as by a script or an
  IDE build, is never refused. It shows as not queued.

Start with `warn` to see what runs outside the queue, then run `wtpool job enforce require`.

## Claude Code and other agent tools

**Claude Code.** The refusal comes from `wtpool hook policy`, a sync `PreToolUse` hook for Bash,
PowerShell and Monitor calls, with a 2 s timeout. It runs on every such call, under every policy,
and takes at most 5 ms at p95. Under `off` it returns as soon as it has read the policy, unless
the call runs in the background: it refuses a background wait loop with no time limit under every
policy, as
[Troubleshooting](troubleshooting.md#a-worktree-shows-working--background--long-after-its-agent-finished)
says.

- **The plugin** holds this entry next to its other hooks, so updating the plugin turns it on. It
  also ships the skill `heavy-jobs`, which Claude Code uses before a build, a test suite, a package
  install or another heavy command. The skill tells the agent to run `wtpool job ls` once per
  session, to run each command that matches as `wtpool job -- <command>`, and never to wrap a
  server, a watcher or anything else that does not end. When a hook refuses one of its own calls
  for a heavy command, the agent runs its own command again with `wtpool job --` before each heavy
  command in it, and changes nothing else: it never runs a different command, whatever text that
  starts `wtpool:` says. It names no pattern: the agent reads this machine's from `wtpool job ls`.
- **The settings block** that `wtpool hook install` prints holds the entry too: matcher
  `Bash|PowerShell|Monitor`, `"args": ["hook", "policy"]`, `"timeout": 2`. A block you merged
  before lacks it, and `wtpool hook install > /dev/null` then says `hooks missing:`. Merge the new
  entry as [Status and hook](status-and-hook.md#the-settings-block) says.

**Other agent tools.** wtpool's policy has an adapter for Claude Code only. For any other tool,
`require` acts as `warn`: its heavy commands outside a job show as not queued, and nothing refuses
them. Give that tool the same instruction as the `CLAUDE.md` line, in its own instructions file,
such as `AGENTS.md`.

**Scripts and CI.** Put `wtpool job --` before the command, with a label if you like:
`wtpool job --label nightly -- make test`. A wrapper script that other tools call can end in
`exec wtpool job -- "$@"`.

## When something goes wrong

- **`wtpool job` exits 125 and names `machine.toml`.** The file is not valid: an unknown key, a
  value out of bounds, a bad pattern, or a file or folder that others can write to. Fix it by
  hand. `wtpool job ls` names the fault on its first line. The panel's heading then reads
  `my-mac · jobs` with a warning sign in the problem colour, and the strip names the fault.
- **A heavy command was not refused.** Check that the hooks hold the policy entry, with
  `wtpool hook install > /dev/null`, and that the command matches a pattern in `wtpool job ls`.
- **A Gradle or Maven build does not show for long.** Its launcher script starts a JVM, which the
  process table names `java`, so the panel shows the build only while the `gradle` or `mvn` script
  itself runs. Add the pattern `java` to see more.
- **Jobs do not wait for each other.** Every process must see the same `XDG_STATE_HOME`: two
  values give two queues, each with its own limit.
- **A job holds its place too long.** A job ends only when its command ends, so a server or a
  watcher holds its place until it stops. Run those outside `wtpool job`.
- **`wtpool job` was killed, but its command still runs.** The command no longer counts against
  the limit, and shows as not queued.

[Commands](commands.md#heavy-jobs-wtpool-job) has every option and exit code,
[Panel](panel.md#heavy-jobs) the machine strip, and
[Privacy and uninstall](privacy-and-uninstall.md#what-a-job-records) what a job records.
