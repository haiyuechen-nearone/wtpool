#![cfg(unix)]
// Always true here. It lets clippy's `*-in-tests` settings cover the helpers too.
#![cfg(test)]

mod support;

mod config;
mod jobs;
mod ls;
mod opencode;
mod refs;
mod slots;
mod startup;
mod status;
mod tools;
