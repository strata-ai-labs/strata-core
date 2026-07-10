//! Hub-URL resolver behavior (resolution-config doc §2/§3): precedence,
//! malformed-source refusal, project-config discovery, hub neutrality.

use strata_hub::{resolve_hub_url, HubUrlError, HubUrlInputs, HubUrlSource};

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
    assert!(matches!(
        resolve_hub_url(&all),
        Err(HubUrlError::NotConfigured)
    ));

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
    assert!(
        matches!(resolve_hub_url(&all), Err(HubUrlError::NotConfigured)),
        "the walk must stop at the nested repo's .git boundary"
    );
}

#[test]
fn unconfigured_refusal_names_the_sources_and_no_default_hub() {
    let error = resolve_hub_url(&inputs()).expect_err("nothing configured");
    let message = error.to_string();
    assert!(message.contains("no hub URL configured"));
    assert!(message.contains("--hub"));
    assert!(message.contains("STRATA_HUB_URL"));
    assert!(message.contains(".strata/config.toml"));
    // Hub neutrality: the refusal (and this crate) never suggests a
    // specific hub as a default.
    assert!(!message.contains("stratahub.io"));
}
