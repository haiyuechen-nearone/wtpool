#![cfg(unix)]
// Always true here. It lets clippy's `*-in-tests` settings cover the helpers too.
#![cfg(test)]

mod support;

mod bare;
mod files;
mod foreground;
mod worktrees;
