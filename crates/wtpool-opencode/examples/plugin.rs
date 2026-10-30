//! Prints the plugin's committed source, `plugin/wtpool.ts`, its `CONFIG` line rendered from the
//! event table for the sample binary.

use wtpool_opencode::plugin::{SAMPLE_BIN, render};

fn main() {
    print!("{}", render(SAMPLE_BIN, false));
}
