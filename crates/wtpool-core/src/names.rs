use std::fmt;

/// A local branch name that `git check-ref-format --branch` accepts.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BranchName(String);

/// Why [`BranchName::try_new`] refused a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[must_use]
pub enum InvalidBranchName {
    #[error("a branch name cannot be empty")]
    Empty,
    #[error("a branch name cannot start with '-'")]
    LeadingDash,
    #[error("'HEAD' is not a valid branch name")]
    Head,
    #[error("a branch name cannot contain {0:?}")]
    ForbiddenChar(char),
    #[error("a branch name cannot contain {0:?}")]
    ForbiddenSequence(&'static str),
    #[error("a branch name cannot start or end with '/'")]
    EdgeSlash,
    #[error("a branch name cannot end with '.'")]
    TrailingDot,
    #[error("a branch name component cannot start with '.' or end with '.lock'")]
    BadComponent,
}

impl BranchName {
    pub fn try_new(name: &str) -> Result<Self, InvalidBranchName> {
        if name.is_empty() {
            return Err(InvalidBranchName::Empty);
        }
        if name.starts_with('-') {
            return Err(InvalidBranchName::LeadingDash);
        }
        if name == "HEAD" {
            return Err(InvalidBranchName::Head);
        }
        if let Some(c) = name
            .chars()
            .find(|&c| c.is_ascii_control() || " ~^:?*[\\".contains(c))
        {
            return Err(InvalidBranchName::ForbiddenChar(c));
        }
        if let Some(sequence) = ["..", "@{", "//"].into_iter().find(|s| name.contains(s)) {
            return Err(InvalidBranchName::ForbiddenSequence(sequence));
        }
        if name.starts_with('/') || name.ends_with('/') {
            return Err(InvalidBranchName::EdgeSlash);
        }
        if name.ends_with('.') {
            return Err(InvalidBranchName::TrailingDot);
        }
        if name
            .split('/')
            .any(|part| part.starts_with('.') || part.ends_with(".lock"))
        {
            return Err(InvalidBranchName::BadComponent);
        }
        Ok(Self(name.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BranchName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The directory name of a worktree under the slot directory: one plain path component.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SlotName(String);

/// Why [`SlotName::try_new`] or [`SlotPrefix::try_new`] refused a name, or why
/// `wtpool new` refused one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[must_use]
pub enum InvalidSlotName {
    #[error("a slot name cannot be empty")]
    Empty,
    #[error("a slot name cannot be '.' or '..'")]
    Dots,
    #[error("a slot name cannot start with '-'")]
    LeadingDash,
    #[error("a slot name cannot contain {0:?}")]
    ForbiddenChar(char),
    /// The name lacks the slot prefix, which the text names with its source, such
    /// as `WTPOOL_SLOT_PREFIX 'app-wt'`.
    #[error("a slot name must start with {0}")]
    MissingPrefix(String),
}

impl SlotName {
    pub fn try_new(name: &str) -> Result<Self, InvalidSlotName> {
        check_component(name)?;
        Ok(Self(name.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SlotName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn check_component(name: &str) -> Result<(), InvalidSlotName> {
    if name.is_empty() {
        return Err(InvalidSlotName::Empty);
    }
    if name == "." || name == ".." {
        return Err(InvalidSlotName::Dots);
    }
    if name.starts_with('-') {
        return Err(InvalidSlotName::LeadingDash);
    }
    if let Some(c) = name.chars().find(|&c| c == '/' || c.is_control()) {
        return Err(InvalidSlotName::ForbiddenChar(c));
    }
    Ok(())
}

/// The prefix that marks a worktree as a managed slot. Slots are named `<prefix><n>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotPrefix(String);

impl SlotPrefix {
    pub fn try_new(prefix: &str) -> Result<Self, InvalidSlotName> {
        check_component(prefix)?;
        Ok(Self(prefix.to_owned()))
    }

    /// The slot name `<prefix><n>`.
    #[must_use]
    pub fn slot(&self, n: u32) -> SlotName {
        // Digits appended to a valid component keep it valid.
        SlotName(format!("{}{n}", self.0))
    }

    /// Whether a worktree directory name marks a managed slot. Like the shell
    /// glob `<prefix>*`, any name starting with the prefix counts.
    #[must_use]
    pub fn manages(&self, dir_name: &str) -> bool {
        dir_name.starts_with(&self.0)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The name of a git remote: what `git remote` lists, never an option or a URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteName(String);

/// Why [`RemoteName::try_new`] refused a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("not a valid git remote name")]
#[must_use]
pub struct InvalidRemoteName;

impl RemoteName {
    /// `name`, refused when git would not accept it as a remote name. git checks
    /// that `refs/remotes/<name>/...` is a valid ref, as the rules for branch names do.
    pub fn try_new(name: &str) -> Result<Self, InvalidRemoteName> {
        BranchName::try_new(name).map_err(|_| InvalidRemoteName)?;
        Ok(Self(name.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for RemoteName {
    /// `origin`, the remote git clone creates.
    fn default() -> Self {
        Self("origin".to_owned())
    }
}

impl fmt::Display for RemoteName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A pull request number, kept as the digits the user typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrNumber(String);

impl PrNumber {
    /// `None` unless `digits` is one or more ASCII digits.
    #[must_use]
    pub fn new(digits: &str) -> Option<Self> {
        (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
            .then(|| Self(digits.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PrNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A GitHub repository, `<owner>/<name>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSlug {
    owner: String,
    name: String,
}

impl RepoSlug {
    /// `None` when either part is empty or contains a `/`.
    #[must_use]
    pub fn new(owner: &str, name: &str) -> Option<Self> {
        let valid = |part: &str| !part.is_empty() && !part.contains('/');
        (valid(owner) && valid(name)).then(|| Self {
            owner: owner.to_owned(),
            name: name.to_owned(),
        })
    }

    /// Compares with another slug, ignoring case as GitHub does.
    #[must_use]
    pub fn matches(&self, other: &RepoSlug) -> bool {
        self.owner.eq_ignore_ascii_case(&other.owner) && self.name.eq_ignore_ascii_case(&other.name)
    }
}

impl fmt::Display for RepoSlug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

/// A full hexadecimal git object id (SHA-1 or SHA-256).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectId(String);

impl ObjectId {
    /// `None` unless `hex`, trimmed, is 40 or 64 hex digits.
    #[must_use]
    pub fn new(hex: &str) -> Option<Self> {
        let hex = hex.trim();
        ((hex.len() == 40 || hex.len() == 64) && hex.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| Self(hex.to_ascii_lowercase()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The first nine hex digits, for messages.
    #[must_use]
    pub fn abbreviated(&self) -> &str {
        self.0.get(..9).unwrap_or(&self.0)
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use rstest::rstest;

    use super::*;

    #[rstest]
    fn branch_name__should_agree_with_git_check_ref_format(
        #[values(
            "main",
            "1234-check-widget-cache-health-before-each-deploy",
            "pr-1234/feature",
            "feature/a.b",
            "@",
            "ünïcode",
            "",
            "-dash",
            "HEAD",
            "a..b",
            "a b",
            "a~1",
            "a^",
            "a:b",
            "a?",
            "a*",
            "a[b",
            "a\\b",
            "a\tb",
            "a\u{7f}b",
            "a@{1}",
            "a//b",
            "/a",
            "a/",
            "a.",
            ".a",
            "a/.b",
            "a.lock",
            "a.lock/b",
            "a/b.lock"
        )]
        sample: &str,
    ) {
        // Given
        let git_accepts = Command::new("git")
            .args(["check-ref-format", "--branch", sample])
            .output()
            .unwrap()
            .status
            .success();

        // When
        let parsed = BranchName::try_new(sample);

        // Then
        assert_eq!(parsed.is_ok(), git_accepts, "{parsed:?}");
    }

    #[test]
    fn branch_name__should_keep_the_text() {
        // Given
        let text = "pr-1234/feature";

        // When
        let name = BranchName::try_new(text).unwrap();

        // Then
        assert_eq!(name.as_str(), text);
        assert_eq!(name.to_string(), text);
    }

    #[rstest]
    #[case::numbered_slot("app-wt1")]
    #[case::other_name("app-review")]
    #[case::inner_dot("a.b")]
    #[case::one_letter("x")]
    fn slot_name__should_accept_a_plain_directory_name(#[case] name: &str) {
        // When
        let slot = SlotName::try_new(name).unwrap();

        // Then
        assert_eq!(slot.as_str(), name);
        assert_eq!(slot.to_string(), name);
    }

    #[rstest]
    #[case::empty("", InvalidSlotName::Empty)]
    #[case::dot(".", InvalidSlotName::Dots)]
    #[case::dot_dot("..", InvalidSlotName::Dots)]
    #[case::leading_dash("-rf", InvalidSlotName::LeadingDash)]
    #[case::slash("a/b", InvalidSlotName::ForbiddenChar('/'))]
    #[case::parent_path("../app", InvalidSlotName::ForbiddenChar('/'))]
    #[case::newline("a\nb", InvalidSlotName::ForbiddenChar('\n'))]
    #[case::nul("a\0b", InvalidSlotName::ForbiddenChar('\0'))]
    fn slot_name__should_reject_anything_but_one_path_component(
        #[case] name: &str,
        #[case] expected: InvalidSlotName,
    ) {
        // When
        let parsed = SlotName::try_new(name);

        // Then
        assert_eq!(parsed, Err(expected));
    }

    #[test]
    fn slot_prefix__should_reject_a_path() {
        // When
        let parsed = SlotPrefix::try_new("../app-wt");

        // Then
        assert_eq!(parsed, Err(InvalidSlotName::ForbiddenChar('/')));
    }

    #[test]
    fn slot_prefix__should_name_slots_prefix_then_number() {
        // Given
        let prefix = SlotPrefix::try_new("app-wt").unwrap();

        // When
        let slot = prefix.slot(3);

        // Then
        assert_eq!(slot.as_str(), "app-wt3");
    }

    #[rstest]
    #[case::numbered("app-wt1", true)]
    #[case::two_digits("app-wt12", true)]
    #[case::any_suffix("app-wt-old", true)]
    #[case::prefix_of_the_prefix("app", false)]
    #[case::other_name("app-review", false)]
    #[case::suffix_only("wt1", false)]
    fn slot_prefix__should_manage_only_names_with_the_prefix(
        #[case] name: &str,
        #[case] expected: bool,
    ) {
        // Given
        let prefix = SlotPrefix::try_new("app-wt").unwrap();

        // When
        let manages = prefix.manages(name);

        // Then
        assert_eq!(manages, expected);
    }

    #[rstest]
    #[case::origin("origin")]
    #[case::with_a_slash("team/upstream")]
    #[case::with_a_dot_inside("my.fork")]
    fn remote_name__should_accept_a_name_git_accepts(#[case] name: &str) {
        // When
        let remote = RemoteName::try_new(name).unwrap();

        // Then
        assert_eq!(remote.as_str(), name);
        assert_eq!(remote.to_string(), name);
    }

    #[rstest]
    #[case::empty("")]
    #[case::option("--upload-pack=x")]
    #[case::url("https://github.com/acme/app.git")]
    #[case::space("my fork")]
    #[case::dot_dot("a..b")]
    fn remote_name__should_refuse_what_git_would_not_take_as_a_remote_name(#[case] name: &str) {
        // When
        let remote = RemoteName::try_new(name);

        // Then
        assert_eq!(
            remote.map_err(|error| error.to_string()),
            Err("not a valid git remote name".to_owned())
        );
    }

    #[test]
    fn remote_name__should_default_to_origin() {
        // When
        let remote = RemoteName::default();

        // Then
        assert_eq!(remote.as_str(), "origin");
    }

    #[rstest]
    #[case::digits("1234", Some("1234"))]
    #[case::leading_zeros("0042", Some("0042"))]
    #[case::empty("", None)]
    #[case::letter("12a", None)]
    #[case::sign("-1", None)]
    #[case::hash("#12", None)]
    fn pr_number__should_accept_only_digits(#[case] digits: &str, #[case] expected: Option<&str>) {
        // When
        let number = PrNumber::new(digits);

        // Then
        assert_eq!(number.as_ref().map(PrNumber::as_str), expected);
        assert_eq!(number.map(|n| n.to_string()).as_deref(), expected);
    }

    #[rstest]
    #[case::same("acme", "widget", true)]
    #[case::other_case("Acme", "WIDGET", true)]
    #[case::other_repository("acme", "widget-private", false)]
    #[case::other_owner("acmecorp", "widget", false)]
    fn repo_slug__should_match_ignoring_case(
        #[case] owner: &str,
        #[case] name: &str,
        #[case] expected: bool,
    ) {
        // Given
        let slug = RepoSlug::new("acme", "widget").unwrap();

        // When
        let matches = slug.matches(&RepoSlug::new(owner, name).unwrap());

        // Then
        assert_eq!(matches, expected);
    }

    #[rstest]
    #[case::empty_owner("", "widget")]
    #[case::empty_name("acme", "")]
    #[case::slash_in_owner("a/b", "widget")]
    #[case::slash_in_name("acme", "a/b")]
    fn repo_slug__should_refuse_an_empty_part_or_a_slash(#[case] owner: &str, #[case] name: &str) {
        // When
        let slug = RepoSlug::new(owner, name);

        // Then
        assert_eq!(slug, None);
    }

    #[rstest]
    #[case::sha1(
        "2F8607391aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        Some("2f8607391aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    )]
    #[case::sha1_with_a_newline(
        "2f8607391aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
        Some("2f8607391aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    )]
    #[case::sha256(
        "2f8607391aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        Some("2f8607391aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    )]
    #[case::abbreviated("2f86073", None)]
    #[case::not_hex("--upload-pack=xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx", None)]
    #[case::empty("", None)]
    fn object_id__should_accept_only_full_hex_ids(
        #[case] hex: &str,
        #[case] expected: Option<&str>,
    ) {
        // When
        let id = ObjectId::new(hex);

        // Then
        assert_eq!(id.as_ref().map(ObjectId::as_str), expected);
        assert_eq!(id.map(|id| id.to_string()).as_deref(), expected);
    }

    #[test]
    fn object_id__should_abbreviate_to_nine_digits() {
        // Given
        let id = ObjectId::new("2f8607391aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();

        // When
        let short = id.abbreviated();

        // Then
        assert_eq!(short, "2f8607391");
    }
}
