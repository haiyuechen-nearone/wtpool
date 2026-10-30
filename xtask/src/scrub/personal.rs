//! The personal data a scrubbed recording never holds.

/// The home folders a scrubbed recording may name: the scrubbers' own.
const SCRUBBED_HOMES: [&str; 2] = ["me", "verify"];

/// What `text` holds that no scrubbed recording may: a home folder but a scrubbed one, an email
/// address, an `sk-` key or a bearer token, each named without its value.
pub(super) fn personal_data(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for root in ["/Users/", "/home/"] {
        for (at, _) in text.match_indices(root) {
            let rest = text.get(at + root.len()..).unwrap_or_default();
            let name: String = rest.chars().take_while(|c| is_word(*c)).collect();
            if !name.is_empty() && !SCRUBBED_HOMES.contains(&name.as_str()) {
                found.push(format!("a home folder {root}…"));
            }
        }
    }
    for (at, _) in text.match_indices('@') {
        let local = text
            .get(..at)
            .unwrap_or_default()
            .chars()
            .rev()
            .take_while(|c| is_word(*c) || "%+".contains(*c))
            .count();
        let domain: String = text
            .get(at + 1..)
            .unwrap_or_default()
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '.')
            .collect();
        let tld = domain.rsplit('.').next().unwrap_or_default();
        if local > 0
            && domain.contains('.')
            && tld.len() >= 2
            && tld.chars().all(|c| c.is_ascii_alphabetic())
        {
            found.push("an email address".to_owned());
        }
    }
    for (marker, what) in [("sk-", "a key"), ("Bearer ", "a bearer token")] {
        for (at, _) in text.match_indices(marker) {
            let token = text
                .get(at + marker.len()..)
                .unwrap_or_default()
                .chars()
                .take_while(|c| is_word(*c))
                .count();
            if token >= 8 {
                found.push(what.to_owned());
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || "._-".contains(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::scrubbed_home(r#"{"HOME":"/Users/me","cwd":"/sandbox/repo/fg"}"#, &[])]
    #[case::verify_home("/home/verify/x", &[])]
    #[case::other_home("/Users/someone/x", &["a home folder /Users/…"])]
    #[case::email("mail someone@example.com", &["an email address"])]
    #[case::not_an_email("@integer @src/credential.ts @@ x@y", &[])]
    #[case::key("sk-abcdefgh12345", &["a key"])]
    #[case::bearer("Bearer abcdefgh.ijk", &["a bearer token"])]
    #[case::short_markers("sk-ab Bearer x", &[])]
    fn personal_data__should_name_what_no_scrubbed_recording_holds(
        #[case] text: &str,
        #[case] expected: &[&str],
    ) {
        // When
        let found = personal_data(text);

        // Then
        assert_eq!(found, expected);
    }
}
