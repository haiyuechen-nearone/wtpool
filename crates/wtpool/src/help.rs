//! The text of `wtpool help`.

pub(crate) const HELP: &str = "\
wtpool - git worktree pool: one foreground worktree you live in, N background
slots agents park branches in.

  wtpool ls                  what is parked where
  wtpool fg   <ref> [--refresh|--reset] [--force]
                             bring a branch/PR into the foreground worktree
  wtpool bg   <ref> [--refresh|--reset]  park a branch/PR in a slot, print its path
  wtpool free <slot|branch> [--force]  release a slot's branch (leaves files, detaches)
  wtpool path <branch>       print the worktree holding <branch>, if any
  wtpool new  [name]         add a slot ([name] must start with slot_prefix)
  wtpool rm   <slot>         remove a slot
  wtpool setup               fill the pool, copy the copy files slots lack
  wtpool config [--json]     print each setting and where it came from
  wtpool status --json       print the pool's worktrees, git facts and builds as JSON
  wtpool status --watch      print that JSON again on each change until stdin closes
  wtpool status --schema     print the JSON schema of that document
  wtpool hook                record a Claude Code hook event from stdin (prints nothing)
  wtpool hook policy         refuse a heavy shell call outside a job, under enforce = require
  wtpool hook gc             delete old activity records
  wtpool hook install        print the hook block for $CLAUDE_CONFIG_DIR/settings.json
                             (default ~/.claude/settings.json)
  wtpool job [--label <text>] [--quiet] -- <command>
                             run a heavy command once fewer heavy jobs than this
                             machine's limit run
  wtpool job ls [--json]     print the limit, policy, heavy commands and jobs
  wtpool job limit [<N>|default]
                             print the limit and its source, or set it
  wtpool job enforce [off|warn|require|default]
                             print the policy for heavy commands outside a job,
                             or set it
  wtpool job heavy [add|rm [--repo] [--] <pattern>...]
                             print the heavy commands, or edit this machine's
                             list, with --repo this repository's
  wtpool job heavy suggest [--json]
                             print the patterns this repository's files suggest;
                             add them with: wtpool job heavy add --repo --suggested
  wtpool --version           print the version, commit and cargo features of this build

<ref> is a PR number, a GitHub PR URL, or a branch name. A PR needs gh and a
remote on github.com.
An existing local branch is reused as is, with a warning when it lags or has
diverged from the remote head.
--refresh fetches and fast-forwards an existing local branch to the remote head.
--reset   also moves a diverged branch (a rebased PR) to the remote head, after
          saving its old tip under refs/wtpool-backup/<branch>/<sha>.
--force   lets fg and free detach or move a worktree where an agent works or
          waits.
bg prints the slot that already holds the branch, if any. It refuses a branch
that the foreground or an unmanaged worktree has checked out. It never makes a
new branch: for one that exists neither here nor on the remote, run
git branch <name> first. fg and bg print only the path on stdout, so
cd \"$(wtpool bg <ref>)\" works; what they did goes to stderr.

Settings foreground, slot_dir, slot_prefix, slot_count and remote each take the
first value set: WTPOOL_FOREGROUND, WTPOOL_SLOT_DIR, WTPOOL_SLOT_PREFIX,
WTPOOL_SLOT_COUNT or WTPOOL_REMOTE, then the repository's
<git common dir>/wtpool.toml, then the default. The repository is the one
WTPOOL_FOREGROUND names, else the one you are in. The defaults are its main
worktree, the folder that holds it, that folder's name plus -wt, 3, and origin.
wtpool.toml may also set copy, files copied from the main worktree into each new
slot when git ignores them, and on_new_slot, a command run there. wtpool config
shows each value and its source.
";
