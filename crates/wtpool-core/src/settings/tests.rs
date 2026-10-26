use std::path::Path;

use crate::config::CopyPath;

use super::*;

mod layers;
mod output;
mod variables;

/// An environment holding only `vars`.
fn env(vars: &[(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
    let vars = vars.to_vec();
    move |name: &str| {
        vars.iter()
            .find(|(var, _)| *var == name)
            .map(|(_, value)| OsString::from(value))
    }
}

fn path(path: &str) -> AbsolutePath {
    AbsolutePath::try_new(path).unwrap()
}

fn values(foreground: Option<&str>, dir: &str, prefix: &str, count: usize) -> Values {
    Values {
        foreground: foreground.map(path),
        slot_dir: Some(path(dir)),
        slot_prefix: Some(SlotPrefix::try_new(prefix).unwrap()),
        slot_count: Some(SlotCount::try_new(count).unwrap()),
        remote: Some(RemoteName::try_new("upstream").unwrap()),
    }
}

fn repo_layer(values: Values) -> RepoLayer {
    RepoLayer {
        file: path("/code/app/.git/wtpool.toml"),
        values,
        repo_only: RepoOnly {
            heavy: Vec::new(),
            build_processes: vec!["make".to_owned()],
            slot_setup: SlotSetup {
                copy: vec![CopyPath::try_new(".envrc").unwrap()],
                on_new_slot: Some("direnv allow".to_owned()),
            },
            bounded_waits: None,
        },
    }
}

fn main_worktree() -> Main {
    Main::Worktree(path("/code/app"))
}

fn bare() -> Main {
    Main::Bare {
        path: path("/code/app.git"),
        file: path("/code/app.git/wtpool.toml"),
    }
}

fn resolve_with(
    vars: &[(&'static str, &'static str)],
    repo: Option<&RepoLayer>,
    main: &Main,
) -> Result<Settings, Error> {
    let env = env::env_values(&env(vars)).unwrap();
    let layers = Layers { env: &env, repo };
    resolve(layers, main, None)
}

fn summary(settings: &Settings) -> [(String, String); 5] {
    [
        (
            settings.foreground.value.display().to_string(),
            settings.foreground.source.to_string(),
        ),
        (
            settings.slot_dir.value.display().to_string(),
            settings.slot_dir.source.to_string(),
        ),
        (
            settings.slot_prefix.value.as_str().to_owned(),
            settings.slot_prefix.source.to_string(),
        ),
        (
            settings.slot_count.value.to_string(),
            settings.slot_count.source.to_string(),
        ),
        (
            settings.remote.value.to_string(),
            settings.remote.source.to_string(),
        ),
    ]
}

fn sample_settings(foreground: Source, repository: Option<&str>) -> Settings {
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
        repository: repository.map(path),
    }
}
