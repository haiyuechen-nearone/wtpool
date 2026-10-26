# Security

## Report a vulnerability

Report it privately through GitHub. Open the repository's **Security** tab and choose **Report a
vulnerability**, or go straight to
<https://github.com/haiyuechen-nearone/wtpool/security/advisories/new>. Do not open a public issue
for it.

Say what an attacker controls, what they gain, and how to reproduce it. A scratch pool, as in
[Development](docs/development.md#try-a-build-on-a-scratch-pool), is the easiest way to show it.
Replies are best effort, with no fixed response time.

## Supported versions

| Version | Supported |
|---|---|
| 0.1, the latest commit of `main` | yes |
| older commits | no |

Fixes land on `main`. There are no release branches yet.

## Threat model

wtpool runs as you, with your permissions, and has no privileged part. It trusts you, the `git`
and `gh` on your `PATH`, and Claude Code. A process that already runs as you can change your
files, settings and `PATH`, so wtpool does not defend against one. The IDs below, such as H3,
point to rows of the spec, [status-panel.md](docs/design/status-panel/status-panel.md), and those
of heavy jobs, such as J4, to [jobs.md](docs/design/jobs/jobs.md).

### The hook runs on every Claude Code event

Claude Code runs `wtpool hook` on 14 events in every session that has the hooks, with the event's
JSON on stdin. The hook fails open, so a broken or slow hook never blocks or changes what the
agent does:

- It prints nothing and exits 0 on every path, panics included (H2, H3). Claude Code reads no
  text and no block decision from it. The one exception is the heavy job policy, below.
- A watchdog ends it after 1 s (H4). The block that `wtpool hook install` prints also sets
  `"onFailure": "continue"` on each entry.
- It runs no shell and no git.
- It parses only the fields it needs. It reads a payload over 32 MiB to its end and drops it.
  Malformed input writes nothing (H5). Session and agent ids must match a fixed pattern, or
  nothing is written (H17).
- Apart from its log (H23), it writes only into an existing `wtpool-activity/` folder that is a
  real folder, not a symlink, and never creates it (H6). On Windows, where wtpool cannot read a
  folder's owner, it does not check that `wtpool-activity/` belongs to you; it relies on the
  folder's permissions. Each write goes to a new temporary file, opened without following
  symlinks, then renamed over the record (H18).
- Stored strings lose control, bidi and zero width characters and keep at most 80 characters
  (H15). Worktree paths and the folder a session started in are the exception: they are stored
  as they are.
- It reads the session's transcript only for its title lines, at most its last 1 MiB, and never
  through a symlink (H16).
- It never opens the output file of a background task (H27). It builds that file's path only
  from checked parts: `CLAUDE_CODE_TMPDIR` or `/tmp` with its symlinks resolved, the user that
  owns `wtpool-activity/`, the transcript's project folder, and the session and task ids. The
  folder and the ids must match fixed patterns. A path with a control, bidi or zero width
  character, or over 1,024 bytes, is not stored.

The plugin runs `wtpool hook` by name, so it runs the first `wtpool` on the `PATH` that Claude
Code's hooks see. The block that `wtpool hook install` prints runs one absolute path instead. On
Windows, a program started by name may be found in the working folder before the `PATH`, so use
the block there.

### The heavy job policy fails open

The hooks also run `wtpool hook policy` before each Bash, PowerShell and Monitor call. It is the
one hook that can stop a call: under `enforce = "require"` it refuses a call whose command holds a
heavy command that `wtpool job` does not wrap, and its reason names the wrapped command to run
instead (H29, J21). It never rewrites a call.

- It exits 0 on every path, and prints at most that one refusal. On any error it prints nothing,
  so the call goes on as it would without the hook: a payload, a config or a command it cannot
  read, no answer before its watchdog, a panic.
- It writes no file and runs no git and no shell. Under `off` it returns once it has read the
  policy.
- Its reason quotes the call's own command back to Claude Code, which sent it. Nothing is stored.

So `require` guides agents that cooperate. It is not a sandbox: a command the hook cannot read, a
heavy command that a script or an editor starts, and an agent tool without this hook all run
unrefused, and show as not queued.

### Heavy job settings

- Only you set the limit and the policy, in `machine.toml`, by hand or through `wtpool job`. No
  environment variable, repository, clone or hook sets them (J4). `limit` or `enforce` in a
  `wtpool.toml` is an error.
- A repository can only add heavy patterns, in its own `.git/wtpool.toml`, which no clone
  carries. It can never lower the limit or relax the policy.
- A repository's files suggest patterns: the names at its top level and the script names of its
  `package.json`, which a hostile repository controls. A suggestion only shows, and applies once
  you add it. A script name that is not a valid pattern is dropped, and a name is only ever the
  last word after its package manager, so a script can make only its own command heavy (J23,
  J24). wtpool reads names and one file of at most 256 KiB, never through a symlink, and runs
  nothing.
- On Unix, `machine.toml` and its folder must belong to you and be writable by no one else (J5).
  The job folder must be a real folder that belongs to you, not a symlink, and writable by no
  one else, or `wtpool job` exits 125 before its command runs (J7). On Windows these checks rely on
  the access list of your profile folder (J17).
- `wtpool job` runs its command as given, with no shell. Its own files open close-on-exec, so the
  command and its children never hold the job's lock (J8).
- `wtpool job` writes `machine.toml` and `wtpool.toml` through a new file, opened without
  following symlinks, renamed over the old one. It refuses to write a file that is a symlink
  (J6).

### Paths can reach the network

No wtpool code opens a network connection: the lints `lint__no_socket_apis` (S2) and
`lint__no_network_modules` (X17) check that. The `git` and `gh` commands that wtpool runs, and
your `on_new_slot`, can use the network, as
[Privacy and uninstall](docs/privacy-and-uninstall.md#what-talks-to-the-network) lists.

But the hook resolves the paths that the model writes in tool calls, such as a Read's `file_path`
or a path word of a Bash command, to find their worktree (H7 to H9). It does that from PreToolUse
on, so before you approve the call. Opening a UNC path such as `\\host\share\file` makes Windows
connect to that host over SMB. Windows may then try to sign in there with your account, which
gives the host an NTLM hash that it can try to crack.

So the hook never stats, opens or resolves a path of these shapes that it reads from a tool
call, from the session's folders, or from a worktree's `.git` file and git directory (H7):

- on Windows, a path with a UNC, device or verbatim prefix, such as `\\host\share`,
  `\\?\UNC\host\share`, `\\.\pipe\name` or `\\?\GLOBALROOT\…`. A drive's prefix, `C:` or
  `\\?\C:`, is the only prefix it reads;
- on Windows, a path with a root but no drive, such as `\file` or `\??\UNC\host\share`;
- elsewhere, a path that starts with exactly two slashes, such as `//host/share/file`, which
  Windows and Cygwin read as a UNC share.

Such a path lies outside every pool. Two cases still make the system contact another machine:

- Where autofs serves `/net`, as on many Linux systems, a path under `/net/<host>` makes the
  system contact that host. Text that the model reads, such as a file in a repository, can lead
  it to write such a path.
- A local path that goes through a symlink whose target is a UNC path: the hook resolves
  symlinks, so Windows then connects to that host.

### `on_new_slot` is a shell command

`on_new_slot` in `wtpool.toml` runs through `sh -c` (`cmd /C` on Windows) in each new slot. It is
the only shell wtpool starts.

- `wtpool.toml` lives in the git common directory, such as `~/code/app/.git/wtpool.toml`. Git
  never reads that file, and no clone, fetch or checkout writes it, so a clone never brings one.
  No environment variable sets `on_new_slot`.
- wtpool refuses a `wtpool.toml` that is a symlink, and rejects unknown keys.
- A repository can hold a whole git directory as plain files, such as `fixtures/app.git`, with a
  `wtpool.toml` in it. By default git uses that directory when you run a command inside it.
  wtpool refuses it when the current folder is inside it, it has no worktree of its own, and it
  lies inside another repository. It stops before it reads that `wtpool.toml`, with
  `could not tell which git repository … belongs to`. This holds for a bare repository and for
  one whose config says `core.bare = false`.
- `WTPOOL_FOREGROUND` set to such a git directory, or to a worktree of it, still selects it. Set
  it only for a repository you trust.
- A git directory outside any other repository, as in an unpacked archive, is not refused.
- `git config --global safe.bareRepository explicit` makes git refuse such git directories
  wherever they are: git then uses a bare repository only when `--git-dir` or `GIT_DIR` names it.
  wtpool then refuses them too. It also stops working with your own bare repositories, because it
  runs git inside the git directory.
- A repository you copy as a folder, with its `.git`, brings its `wtpool.toml` along. Read
  `.git/wtpool.toml` before you run wtpool in a copy that someone else made.
- `copy` copies only files that git ignores in the main worktree, and never through a symlink.
  Those files, such as `.env`, then exist in every slot your agents work in.

### The CLI's output

A branch name can hold characters that a terminal acts on, such as a C1 control that starts an
OSC 52 sequence, which writes your clipboard.

- The CLI shows each control, bidi, zero width or other invisible format character of a branch,
  path or other string from git, a record or a setting as `\u{XXXX}`, in every line it writes
  itself (R2).
- Two outputs keep such strings as they are: its JSON, and the path that `fg`, `bg` or `path`
  prints when its standard output is not a terminal, so that `cd "$(wtpool bg <ref>)"` names the
  real folder.
- The output of the `git` commands it runs, of `on_new_slot` and of the command that `wtpool job`
  runs passes through as it is.

### The VS Code extension

- Its manifest declares untrusted workspaces unsupported, so VS Code keeps it disabled in
  Restricted Mode, and it runs `wtpool`, and through it git, only in a workspace you trust.
- The tab's page runs under a strict Content Security Policy (X8): `default-src 'none'`, scripts
  only with a nonce made for each load, styles and fonts only from the extension, and Trusted
  Types required. It has no inline styles.
- Every string from git, records or command output reaches the page only through `textContent`
  or `setAttribute` (X10). The page has no links and builds no HTML from data. Control and format
  characters, the bidi and zero width ones among them, the line and paragraph separators and the
  tag block show as `\u{XXXX}`, also those that the output tail keeps. The output tail and Follow
  show a tab as spaces.
- Command URIs and forms are off, and the page loads resources only from the extension's
  `media/` folder (X9).
- The extension accepts only a fixed set of messages from the page, and each must name a row or
  output path of the last status document (X11). A `jobSettings` message, from the machine
  strip, must hold a limit within bounds, a known policy, or one pattern edit that passes the
  pattern grammar. The extension then runs only `wtpool job limit`, `enforce` or `heavy add|rm`,
  with each pattern as one argument after `--`, and writes no file itself (X21).
- It runs only the binary that `wtpool.path` names, with an argument array and no shell (X4).
  That setting has application scope, so a workspace's `.vscode/settings.json` cannot change it.
  A bare name, such as the default `wtpool`, runs the first match in an absolute folder of the
  `PATH`. The workspace folder and relative `PATH` entries are never searched.
- It loads no networking module and runs no server (X17).
- The output tail and Follow read only paths that the status document lists, never through a
  symlink, and only regular files (R12). `wtpool status` lists a record's output path only when
  it is absolute, with no `.` or `..` part, and the extension reads it only when it ends in
  `tasks/<id>.output`. Neither checks where that path points. So a record that the hook did not
  write, such as one in a copied `.git`, can name any file you can read whose path ends so, and
  the panel then shows its last lines.
- The output tail drops every escape sequence and C1 control (R12). Follow drops them too, keeps
  only SGR colour sequences, and drops every other control character but tab, line feed and
  carriage return (X19). So a build log cannot write your clipboard (OSC 52) or show a disguised
  link (OSC 8). Follow passes bidi, zero width and other format characters to the terminal as
  they are. The terminal's name leaves out every invisible character.

### What wtpool stores

wtpool stores paths and labels, never prompt text or command output. A record holds worktree
paths, the paths of files an agent works on, short command summaries such as `cargo test`, and
the path of a background task's output file.

A Bash call's summary is its program's name. For a few programs, such as `git` and `cargo`, the
next word joins it when it is a lower case word of at most 24 characters (H14). No other word of
the command is stored, and nothing inside `$(…)` or backquotes reaches the summary.

Session titles can reflect prompt content. Claude Code generates a title from your conversation,
and wtpool keeps up to 40 characters of it as the session's label. The panel labels a subagent
with the description that its parent agent wrote in the Agent call.

A heavy job records its summary, label, folder, pid and times, never its arguments, environment
or output (J8, J16). Its folder has mode 0700, its files 0600.

[Privacy and uninstall](docs/privacy-and-uninstall.md) lists every file and field.

### Test runs

The live contract run of the verify harness, `cargo xtask verify --phase 3 --part cc --contract`,
starts real Claude Code sessions on your login, and their model calls cost money. It runs the
`claude` on your `PATH`, and the harness supports only the latest installed Claude Code. The
sessions are locked down:

- A script answers their permission prompts and allows only the calls each session's script
  names. Calls that Claude Code allows without a prompt, such as a Read inside the session's
  pool, never reach it. The contract turn has no tools.
- Claude Code's sandbox runs their Bash commands: they write only inside the run folder and
  Claude Code's temporary folder, and reach no host but localhost. WebFetch and WebSearch are
  denied.
- They see your home folder, as macOS finds your login keychain through it, but only an
  allowlist of your environment variables, so no `GH_TOKEN`, SSH agent or cloud token. No
  claude.ai connector of your account loads.
- They load only the run's settings file. Claude Code's configuration, data, cache and state
  folders are in the run folder, and its auto-updater is off.
- `contract__should_run_in_the_sandbox` fails when your Claude Code install changed during the
  run.

No other part needs a login. [CONTRIBUTING](CONTRIBUTING.md#the-verify-harness) says when you
need that run, and the spec's
[Contract test](docs/design/status-panel/verification.md#contract-test) says how its sessions are
kept apart from your own setup.
