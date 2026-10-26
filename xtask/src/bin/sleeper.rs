//! A real process of the int checks (`builds__should_match_cargo_star_and_rustc`): copied under
//! a build's name, such as `cargo-verify`, it waits for the seconds of its first argument, 60 by
//! default, in the folder it was started in. A copy of the system's own `sleep` would do on most
//! systems, but macOS kills a copy of a platform binary at its start.

use std::time::Duration;

fn main() {
    let seconds = std::env::args()
        .nth(1)
        .and_then(|text| text.parse().ok())
        .unwrap_or(60);
    std::thread::sleep(Duration::from_secs(seconds));
}
