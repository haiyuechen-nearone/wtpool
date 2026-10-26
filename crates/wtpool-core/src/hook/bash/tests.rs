use rstest::rstest;

use super::*;

const CWD: &str = "/p/fg";
const HOME: &str = "/h";

#[rstest]
#[case::single_path("ls /p/wt2", &[], &["/p/wt2"], "ls")]
#[case::cd_then_program("cd /p/wt2 && ls /p/wt3", &["/p/wt2"], &["/p/wt3"], "ls")]
#[case::relative_cd("cd ../wt3 && cat f.txt", &["/p/fg/../wt3"], &[], "cat")]
#[case::relative_word_after_cd("cd /p/wt1; cat a/b", &["/p/wt1"], &["/p/wt1/a/b"], "cat")]
#[case::bare_cd("cd", &["/h"], &[], "cd")]
#[case::escaped_quote("cat \"/p/wt2/a\\\"b\"", &[], &["/p/wt2/a\"b"], "cat")]
#[case::append_redirect("echo x >> /p/wt2/log", &[], &["/p/wt2/log"], "echo")]
#[case::heredoc_after_a_space("cat << END\n/p/wt3/f\nEND", &[], &[], "cat")]
#[case::time_option("time -p make /p/wt1/m", &[], &["/p/wt1/m"], "make")]
#[case::git_dash_c("git -C wt1 status", &[], &["/p/fg/wt1"], "git")]
#[case::manifest_path("cargo test --manifest-path=/p/wt3/C.toml", &[], &["/p/wt3/C.toml"], "cargo test")]
#[case::sh_dash_c("sh -c \"ls /p/wt2\"", &[], &["/p/wt2"], "sh")]
#[case::nix_develop("nix develop -c cargo build", &[], &[], "cargo build")]
#[case::nix_without_command("nix develop .#x", &[], &[], "nix develop")]
#[case::tilde("cat ~/wt2/f", &[], &["/h/wt2/f"], "cat")]
#[case::home_variable("cat $HOME/wt3/f ${HOME}/x", &[], &["/h/wt3/f", "/h/x"], "cat")]
#[case::redirect_target("cargo build 2>&1 | tee /p/wt1/b.log", &[], &["/p/wt1/b.log"], "cargo build")]
#[case::output_redirect("echo hi > /p/wt2/n &>> /p/wt3/m", &[], &["/p/wt2/n", "/p/wt3/m"], "echo")]
#[case::heredoc_body("cat > /tmp/n <<'EOF'\n/p/wt3/f\nEOF", &[], &["/tmp/n"], "cat")]
#[case::heredoc_with_tabs("cat <<-END\n\t/p/wt3/f\n\tEND\nls /p/wt1", &[], &["/p/wt1"], "cat")]
#[case::here_string("cat <<< /p/wt2/x", &[], &["/p/wt2/x"], "cat")]
#[case::url_and_option("curl -I/x https://u:p@h/x", &[], &[], "curl")]
#[case::escaped_space("cat /p/wt2/a\\ b", &[], &["/p/wt2/a b"], "cat")]
#[case::assignment("TOKEN=s cargo build", &[], &[], "cargo build")]
#[case::env_prefix("env -i A=1 npm run build", &[], &[], "npm run")]
#[case::timeout_prefix("timeout -s KILL 600 cargo test", &[], &[], "cargo test")]
#[case::time_and_nice("time nice -n 5 make check", &[], &[], "make")]
#[case::basename("/usr/bin/git status", &[], &[], "git status")]
#[case::capital_word("git Status", &[], &[], "git")]
#[case::long_word("npm abcdefghijklmnopqrstuvwxy", &[], &[], "npm")]
#[case::option_first("gh --token s auth", &[], &[], "gh")]
#[case::unlisted("make test", &[], &[], "make")]
#[case::or_separator("false || ls /p/x", &[], &["/p/x"], "false")]
#[case::empty("", &[], &[], "")]
#[case::substitution_in_assignment("D=$(ls -d ~/x | head -1); echo $D", &[], &["/h/x"], "echo")]
#[case::secret_in_substitution("TOKEN=$(echo ghp_abc) gh auth login", &[], &[], "gh auth")]
#[case::secret_in_backquotes("TOKEN=`echo ghp_abc` gh auth", &[], &[], "gh auth")]
#[case::backquotes_in_assignment("D=`ls /p/wt2`; echo $D", &[], &["/p/wt2"], "echo")]
#[case::assignments_only("A=1 B=$(make x)", &[], &[], "")]
#[case::append_assignment("TOKEN+=s cargo build", &[], &[], "cargo build")]
#[case::array_assignment("A=(ghp_abc /p/wt2/x); make", &[], &[], "make")]
#[case::subshell_cd("(cd /p/wt2 && make)", &["/p/wt2"], &[], "make")]
#[case::subshell_folder_ends("(cd /p/wt2; cat a/b); cat c/d", &["/p/wt2"], &["/p/wt2/a/b", "/p/fg/c/d"], "cat")]
#[case::group_keeps_folder("{ cd /p/wt1; make; }; cat a/b", &["/p/wt1"], &["/p/wt1/a/b"], "make")]
#[case::cd_in_substitution("X=$(cd /p/wt3 && pwd)", &[], &["/p/wt3"], "")]
#[case::cd_in_substitution_keeps_folder("X=$(cd /p/wt3); cat a/b", &[], &["/p/wt3", "/p/fg/a/b"], "cat")]
#[case::cd_to_substitution("cd $(git rev-parse --show-toplevel) && cat a/b", &[], &["/p/fg/a/b"], "cat")]
#[case::arithmetic("echo $((1+2)) $((x/2)) /p/wt3", &[], &["/p/wt3"], "echo")]
#[case::arithmetic_command("((i++)); ls /p/wt2", &[], &["/p/wt2"], "ls")]
#[case::nested_substitution("echo $(a $(b /p/wt2/x))", &[], &["/p/wt2/x"], "echo")]
#[case::quotes_around_substitution("cat \"$(echo \")\")\" /p/wt3/x", &[], &["/p/wt3/x"], "cat")]
#[case::substitution_in_quotes("cat \"$(ls /p/wt2) )\"", &[], &["/p/wt2"], "cat")]
#[case::single_quoted_substitution("echo '$(ls /p/wt2)'", &[], &[], "echo")]
#[case::process_substitution("diff <(ls /p/wt2) /p/wt3/x", &[], &["/p/wt2", "/p/wt3/x"], "diff")]
#[case::program_from_substitution("$(which git) status", &[], &[], "")]
#[case::program_from_backquotes("`which git` status; ls", &[], &[], "ls")]
#[case::program_with_option("-d /p/wt2/x", &[], &["/p/wt2/x"], "")]
#[case::program_with_parenthesis("@(ls|cat) /p/x", &[], &["/p/x"], "")]
#[case::comment("# don't\ncargo build # /p/wt2", &[], &[], "cargo build")]
#[case::hash_inside_word("git log --grep=#1 a/b", &[], &["/p/fg/a/b"], "git log")]
#[case::backslash_in_double_quotes("cat \"/p/wt2/a\\b\"", &[], &["/p/wt2/a\\b"], "cat")]
#[case::ansi_c_quotes("cat $'/p/wt2/a\\'b'", &[], &["/p/wt2/a'b"], "cat")]
#[case::braces_word("find /p/x -exec rm {} +", &[], &["/p/x"], "find")]
fn parse__should_find_paths_cds_and_summary(
    #[case] command: &str,
    #[case] cds: &[&str],
    #[case] paths: &[&str],
    #[case] summary: &str,
) {
    // When
    let parsed = parse(command, Path::new(CWD), Some(Path::new(HOME)));

    // Then
    let to_paths = |list: &[&str]| list.iter().map(PathBuf::from).collect::<Vec<_>>();
    assert_eq!(parsed.cds, to_paths(cds), "cds");
    assert_eq!(parsed.paths, to_paths(paths), "paths");
    assert_eq!(parsed.summary, summary, "summary");
}

#[test]
fn parse__should_keep_words_without_a_home() {
    // When
    let parsed = parse("cat ~/x", Path::new(CWD), None);

    // Then
    assert_eq!(parsed.paths, vec![PathBuf::from("/p/fg/~/x")]);
}

#[rstest]
#[case::nested("a $(b $(c d)) e", &["a", "$(b $(c d))", "e"], &["b $(c d)"])]
#[case::arithmetic("x $((1+2))", &["x", "$((1+2))"], &[])]
#[case::quoted_parenthesis("\"$(echo \")\")\"", &["$(echo \")\")"], &["echo \")\""])]
#[case::backquotes("`a \\`b\\``", &["`a \\`b\\``"], &["a `b`"])]
#[case::parameter("${A:-x y} z", &["${A:-x y}", "z"], &[])]
#[case::unclosed("a $(b 'c", &["a", "$(b 'c)"], &["b 'c"])]
fn tokenize__should_keep_substitutions_in_their_word(
    #[case] command: &str,
    #[case] words: &[&str],
    #[case] substitutions: &[&str],
) {
    // When
    let tokens = lex::tokenize(command);

    // Then
    let found: Vec<&lex::Word> = tokens
        .iter()
        .filter_map(|token| match token {
            Token::Word(word) => Some(word),
            _ => None,
        })
        .collect();
    let texts: Vec<&str> = found.iter().map(|w| w.text.as_str()).collect();
    let inner: Vec<&str> = found
        .iter()
        .flat_map(|w| w.substitutions.iter().map(String::as_str))
        .collect();
    assert_eq!(texts, words, "words");
    assert_eq!(inner, substitutions, "substitutions");
}

#[test]
fn parse__should_survive_deep_nesting() {
    // Given
    let command = format!("{}x{}", "$(".repeat(100_000), ")".repeat(100_000));

    // When
    let parsed = parse(&format!("echo {command} /p/wt2"), Path::new(CWD), None);

    // Then
    assert_eq!(parsed.summary, "echo");
    assert_eq!(parsed.paths, vec![PathBuf::from("/p/wt2")]);
}
