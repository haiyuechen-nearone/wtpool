#![cfg(unix)]
// Always true here. It lets clippy's `*-in-tests` settings cover the helpers too.
#![cfg(test)]

mod states;
mod support;

mod background;
mod copy;
mod foreground;
mod free;
mod held_elsewhere;
mod list;
mod lock;
mod new;
mod on_new_slot;
mod path;
mod remote;
mod remove;
mod resolve;
mod setup;
mod sync;
mod tracking;
