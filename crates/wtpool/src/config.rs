//! The text and the JSON of `wtpool config`: each setting with its value and its source.

use wtpool_core::{Settings, Value};

/// One line per setting under a header: name, value and source, in columns.
pub fn text(settings: &Settings) -> String {
    let mut rows = vec![[
        "SETTING".to_owned(),
        "VALUE".to_owned(),
        "SOURCE".to_owned(),
    ]];
    rows.extend(
        settings
            .rows()
            .map(|(name, value, source)| [name.to_owned(), value.to_string(), source.to_string()]),
    );
    let name_width = rows.iter().map(|[name, ..]| name.chars().count()).max();
    let value_width = rows.iter().map(|[_, value, _]| value.chars().count()).max();
    let (name_width, value_width) = (name_width.unwrap_or(0), value_width.unwrap_or(0));
    rows.iter()
        .map(|[name, value, source]| {
            format!("{name:<name_width$}  {value:<value_width$}  {source}\n")
        })
        .collect()
}

/// The settings as a JSON object: `version`, then each setting with its `value` and its
/// `source` as [`text`] shows it.
pub fn json(settings: &Settings) -> String {
    let mut json = "{\n  \"version\": 1".to_owned();
    for (name, value, source) in settings.rows() {
        let value = match value {
            Value::Text(text) => json_string(&text),
            Value::Number(number) => number.to_string(),
        };
        json.push_str(&format!(
            ",\n  \"{name}\": {{\"value\": {value}, \"source\": {}}}",
            json_string(&source.to_string())
        ));
    }
    json.push_str("\n}\n");
    json
}

/// `text` as a JSON string.
fn json_string(text: &str) -> String {
    // Serializing a `str` into memory cannot fail. `null` keeps the document valid JSON
    // all the same.
    serde_json::to_string(text).unwrap_or_else(|_| "null".to_owned())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use wtpool_core::{
        AbsolutePath, CopyPath, RemoteName, Setting, SlotCount, SlotPrefix, SlotSetup, Source,
    };

    use super::*;

    fn path(path: &str) -> AbsolutePath {
        AbsolutePath::try_new(path).unwrap()
    }

    fn sample_settings(foreground: Source) -> Settings {
        Settings {
            foreground: Setting::new(path("/code/app"), foreground),
            slot_dir: Setting::new(path("/code"), Source::MainWorktreeParent),
            slot_prefix: Setting::new(
                SlotPrefix::try_new("app-wt").unwrap(),
                Source::MainWorktreeName,
            ),
            slot_count: Setting::new(
                SlotCount::default(),
                Source::RepoFile(PathBuf::from("/code/app/.git/wtpool.toml")),
            ),
            remote: Setting::new(RemoteName::default(), Source::Env("WTPOOL_REMOTE")),
            heavy: Vec::new(),
            build_processes: Vec::new(),
            slot_setup: SlotSetup {
                copy: vec![CopyPath::try_new(".env").unwrap()],
                on_new_slot: None,
            },
            main_worktree: Some(path("/code/app")),
            repository: Some(path("/code/app/.git")),
        }
    }

    #[test]
    fn text__should_align_each_setting_with_its_source() {
        // Given
        let settings = sample_settings(Source::MainWorktree);

        // When
        let text = text(&settings);

        // Then
        assert_eq!(
            text,
            "SETTING      VALUE      SOURCE\n\
             foreground   /code/app  main worktree\n\
             slot_dir     /code      parent of the main worktree\n\
             slot_prefix  app-wt     main worktree name + -wt\n\
             slot_count   3          /code/app/.git/wtpool.toml\n\
             remote       origin     env WTPOOL_REMOTE\n"
        );
    }

    #[test]
    fn json__should_give_each_setting_its_value_and_source() {
        // Given
        let settings = sample_settings(Source::Default);

        // When
        let json = json(&settings);

        // Then
        assert_eq!(
            json,
            "{\n  \"version\": 1,\n  \
             \"foreground\": {\"value\": \"/code/app\", \"source\": \"default\"},\n  \
             \"slot_dir\": {\"value\": \"/code\", \"source\": \"parent of the main worktree\"},\n  \
             \"slot_prefix\": {\"value\": \"app-wt\", \"source\": \"main worktree name + -wt\"},\n  \
             \"slot_count\": {\"value\": 3, \"source\": \"/code/app/.git/wtpool.toml\"},\n  \
             \"remote\": {\"value\": \"origin\", \"source\": \"env WTPOOL_REMOTE\"}\n}\n"
        );
    }

    #[test]
    fn json_string__should_escape_the_characters_json_does_not_allow_in_a_string() {
        // When
        let json = json_string("a\"b\\c\nd\te\u{1}é\u{7f}");

        // Then
        assert_eq!(json, "\"a\\\"b\\\\c\\nd\\te\\u0001é\u{7f}\"");
    }
}
