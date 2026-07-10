//! Hub-URL resolver behavior (resolution-config doc §2/§3): precedence,
//! malformed-source refusal, project-config discovery, the built-in
//! default.

use strata_hub::{resolve_hub_url, HubUrlError, HubUrlInputs, HubUrlSource, DEFAULT_HUB_URL};

fn inputs() -> HubUrlInputs {
    HubUrlInputs::default()
}

#[test]
fn precedence_is_flag_env_project_global() {
    let workdir = tempfile::tempdir().expect("tempdir");
    let project = workdir.path().join(".strata");
    std::fs::create_dir_all(&project).expect("mkdir");
    std::fs::write(
        project.join("config.toml"),
        "[hub]\nurl = \"https://project.example.com\"\n",
    )
    .expect("write");
    let global = workdir.path().join("global.toml");
    std::fs::write(&global, "[hub]\nurl = \"https://global.example.com\"\n").expect("write");

    let mut all = HubUrlInputs {
        flag: Some("https://flag.example.com".to_owned()),
        environment: Some("https://env.example.com".to_owned()),
        working_dir: Some(workdir.path().to_owned()),
        global_config: Some(global),
    };

    let resolved = resolve_hub_url(&all).expect("resolves");
    assert_eq!(resolved.url.as_str(), "https://flag.example.com/");
    assert_eq!(resolved.source, HubUrlSource::Flag);

    all.flag = None;
    let resolved = resolve_hub_url(&all).expect("resolves");
    assert_eq!(resolved.url.as_str(), "https://env.example.com/");
    assert_eq!(resolved.source, HubUrlSource::Environment);

    all.environment = None;
    let resolved = resolve_hub_url(&all).expect("resolves");
    assert_eq!(resolved.url.as_str(), "https://project.example.com/");
    assert!(matches!(resolved.source, HubUrlSource::ProjectConfig(_)));

    all.working_dir = None;
    let resolved = resolve_hub_url(&all).expect("resolves");
    assert_eq!(resolved.url.as_str(), "https://global.example.com/");
    assert!(matches!(resolved.source, HubUrlSource::GlobalConfig(_)));
}

#[test]
fn empty_env_is_unset_but_whitespace_and_bad_urls_abort_naming_the_source() {
    let mut all = inputs();
    all.environment = Some(String::new());
    let resolved = resolve_hub_url(&all).expect("empty env is unset");
    assert_eq!(resolved.source, HubUrlSource::Default);

    all.environment = Some("   ".to_owned());
    let Err(HubUrlError::MalformedSource { source, .. }) = resolve_hub_url(&all) else {
        panic!("whitespace-only env must abort");
    };
    assert_eq!(source, "STRATA_HUB_URL");

    let mut flagged = inputs();
    flagged.flag = Some("not a url".to_owned());
    let Err(HubUrlError::MalformedSource { source, .. }) = resolve_hub_url(&flagged) else {
        panic!("bad flag must abort");
    };
    assert_eq!(source, "--hub");
}

#[test]
fn malformed_project_config_never_falls_through() {
    let workdir = tempfile::tempdir().expect("tempdir");
    let project = workdir.path().join(".strata");
    std::fs::create_dir_all(&project).expect("mkdir");
    std::fs::write(project.join("config.toml"), "not toml [").expect("write");
    let global = workdir.path().join("global.toml");
    std::fs::write(&global, "[hub]\nurl = \"https://global.example.com\"\n").expect("write");

    let all = HubUrlInputs {
        flag: None,
        environment: None,
        working_dir: Some(workdir.path().to_owned()),
        global_config: Some(global),
    };
    assert!(
        matches!(
            resolve_hub_url(&all),
            Err(HubUrlError::MalformedSource { .. })
        ),
        "a broken project config aborts instead of using the global"
    );
}

#[test]
fn project_config_walk_stops_at_a_git_boundary() {
    let workdir = tempfile::tempdir().expect("tempdir");
    // Repo root has the config; a NESTED repo below it must not see it.
    let outer_strata = workdir.path().join(".strata");
    std::fs::create_dir_all(&outer_strata).expect("mkdir");
    std::fs::write(
        outer_strata.join("config.toml"),
        "[hub]\nurl = \"https://outer.example.com\"\n",
    )
    .expect("write");

    let nested = workdir.path().join("nested-repo/deep");
    std::fs::create_dir_all(&nested).expect("mkdir");
    std::fs::create_dir_all(workdir.path().join("nested-repo/.git")).expect("git marker");

    let all = HubUrlInputs {
        working_dir: Some(nested),
        ..inputs()
    };
    let resolved = resolve_hub_url(&all).expect("resolves");
    assert_eq!(
        resolved.source,
        HubUrlSource::Default,
        "the walk must stop at the nested repo's .git boundary and fall \
         through to the default"
    );
}

#[test]
fn a_config_file_without_the_key_is_unset_not_malformed() {
    // The exact `strata config set` + `unset` residue: the file exists
    // with an empty [hub] table. Resolution falls through to the
    // default instead of aborting.
    let workdir = tempfile::tempdir().expect("tempdir");
    let global = workdir.path().join("global.toml");
    std::fs::write(&global, "[hub]\n").expect("write");
    let all = HubUrlInputs {
        global_config: Some(global.clone()),
        ..inputs()
    };
    let resolved = resolve_hub_url(&all).expect("resolves");
    assert_eq!(resolved.source, HubUrlSource::Default);

    // A key that is present but not a string still aborts.
    std::fs::write(&global, "[hub]\nurl = 7\n").expect("write");
    let all = HubUrlInputs {
        global_config: Some(global),
        ..inputs()
    };
    assert!(matches!(
        resolve_hub_url(&all),
        Err(HubUrlError::MalformedSource { .. })
    ));
}

#[test]
fn nothing_configured_resolves_to_the_official_hub() {
    let resolved = resolve_hub_url(&inputs()).expect("the default always resolves");
    assert_eq!(resolved.source, HubUrlSource::Default);
    // The exported const is the single source of truth; asserting through
    // it keeps the host literal out of this file (single-surface rule).
    assert_eq!(
        resolved.url.as_str().trim_end_matches('/'),
        DEFAULT_HUB_URL,
        "the fallback is the built-in default"
    );
    assert_eq!(resolved.source.to_string(), "built-in default");
}
