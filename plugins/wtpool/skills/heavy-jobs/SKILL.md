---
name: heavy-jobs
description: Use before running a build, a test suite, a package install or any other heavy command, to run it under this machine's limit on heavy jobs.
---
Run `wtpool job ls` once per session. Its first lines name this machine's heavy commands, the
policy and the wrapper. Run each command that matches as `wtpool job -- <command>`, for example
`wtpool job -- cargo test`; a label helps: `wtpool job --label "unit tests" -- npm test`.
A command inside `wtpool job` needs nothing more. When a call is refused with a reason that
starts `wtpool:`, run the command that reason gives. Never wrap a server, a watcher or anything
else that does not end.
