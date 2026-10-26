//! Prints the hook file of the wtpool Claude Code plugin, `plugins/wtpool/hooks/hooks.json`.

fn main() {
    print!("{}", wtpool_claude_code::plugin_hooks_file());
}
