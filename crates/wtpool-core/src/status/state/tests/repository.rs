//! The name and GitHub slug of the pool's repository.

use super::*;

#[test]
fn document__should_name_the_repository_of_the_pool() {
    // Given
    let fixture = must_fixture();
    let local = fixture.must_load(Vec::new());
    must_git(
        &fixture.main,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/acme/app.git",
        ],
    );
    let mut github = fixture.must_load(Vec::new());
    must_git(&fixture.main, &["remote", "remove", "origin"]);
    let before = github.document(Timestamp::from_seconds(0));

    // When
    github
        .refresh_all(&FixedProcesses::new(Vec::new()))
        .unwrap();

    // Then
    let repository = |state: &PoolState| {
        state.document(Timestamp::from_seconds(0)).pools[0]
            .repository
            .clone()
    };
    assert_eq!(
        repository(&local),
        Repository {
            name: "main".to_owned(),
            github: None
        }
    );
    assert_eq!(
        before.pools[0].repository.github.as_deref(),
        Some("acme/app")
    );
    assert_eq!(repository(&github).github, None);
}

#[test]
fn document__should_read_the_url_of_the_configured_remote() {
    // Given
    let fixture = must_fixture();
    let url = "https://github.com/acme/app.git";
    must_git(&fixture.main, &["remote", "add", "upstream", url]);
    let config = Config {
        remote: RemoteName::try_new("upstream").unwrap(),
        ..fixture.config()
    };
    let processes = FixedProcesses::new(Vec::new());

    // When
    let state =
        PoolState::load(config, fixture.common.clone(), &processes, "m".to_owned()).unwrap();

    // Then
    let document = state.document(Timestamp::from_seconds(0));
    assert_eq!(
        document.pools[0].repository.github.as_deref(),
        Some("acme/app")
    );
}
