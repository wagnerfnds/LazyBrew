use lazybrew::{
    backend::{cli::operation_args, parser},
    domain::*,
};
#[test]
fn real_installed_fixture() {
    let packages = parser::info(include_bytes!("fixtures/homebrew/installed.json")).unwrap();
    assert!(!packages.is_empty());
    assert!(packages.iter().any(|p| p.package.id.name() == "atuin"));
    assert!(packages.iter().all(|p| !p.package.installed.is_empty()));
}
#[test]
fn real_package_details() {
    let formula = parser::info(include_bytes!("fixtures/homebrew/info-formula.json")).unwrap();
    assert_eq!(formula[0].package.id.name(), "jq");
    assert_eq!(formula[0].package.id.kind, PackageKind::Formula);
    assert!(!formula[0].homepage.is_empty());
    let cask = parser::info(include_bytes!("fixtures/homebrew/info-cask.json")).unwrap();
    assert_eq!(cask[0].package.id.name(), "firefox");
    assert_eq!(cask[0].package.id.kind, PackageKind::Cask);
    assert!(cask[0].package.installed.is_empty());
}
#[test]
fn real_services() {
    let services = parser::services(include_bytes!("fixtures/homebrew/services.json")).unwrap();
    assert_eq!(services[0].name, "atuin");
    let info = parser::services(include_bytes!("fixtures/homebrew/service-info.json")).unwrap();
    assert_eq!(info[0].running, Some(true));
}
#[test]
fn outdated_empty_and_populated() {
    assert!(
        parser::outdated(include_bytes!("fixtures/homebrew/outdated.json"))
            .unwrap()
            .is_empty()
    );
    let bytes = br#"{"formulae":[{"name":"php@8.3","installed_versions":["8.3.0"],"current_version":"8.3.1","pinned":true}],"casks":[{"name":"firefox","installed_versions":["100"],"current_version":"101"}]}"#;
    let packages = parser::outdated(bytes).unwrap();
    assert_eq!(packages.len(), 2);
    assert!(packages[0].pinned);
    assert_eq!(packages[1].id.kind, PackageKind::Cask);
    assert!(packages.iter().all(|p| p.outdated));
}
#[test]
fn malformed_json_is_not_silently_empty() {
    for bytes in [b"not json".as_slice(), b"{}", b"{\"formulae\": []}"] {
        assert!(parser::info(bytes).is_err());
    }
}
#[test]
fn reject_flags_paths_and_shell_syntax() {
    for name in [
        "",
        "--force",
        "-x",
        "foo;touch",
        "$(id)",
        "foo\nbar",
        "/tmp/pkg",
        "../pkg",
        "https://host/a",
        "a/b",
        "a/../b",
        "foo bar",
        "foo`id`",
        "a\\b",
    ] {
        assert!(
            PackageId::new(name, PackageKind::Formula).is_err(),
            "accepted {name}"
        );
    }
    for name in ["php@8.3", "node", "openssl@3", "owner/tap/formula", "c++"] {
        assert!(PackageId::new(name, PackageKind::Formula).is_ok());
    }
}
#[test]
fn commands_are_typed_and_unambiguous() {
    let id = PackageId::new("firefox", PackageKind::Cask).unwrap();
    assert_eq!(
        operation_args(&Operation::Install(id.clone())).unwrap(),
        ["install", "--cask", "firefox"]
    );
    assert!(operation_args(&Operation::Pin(id.clone())).is_err());
    assert!(operation_args(&Operation::Start(id)).is_err());
    let id = PackageId::new("owner/tap/php@8.3", PackageKind::Formula).unwrap();
    assert_eq!(
        operation_args(&Operation::Restart(id)).unwrap(),
        ["services", "restart", "owner/tap/php@8.3"]
    );
}
#[test]
fn search_handles_headers_and_marks() {
    let packages = parser::search("==> Formulae\njq ✔\nyq\n", PackageKind::Formula);
    assert_eq!(
        packages.iter().map(|p| p.id.name()).collect::<Vec<_>>(),
        ["jq", "yq"]
    );
}
#[test]
fn cask_installed_variants_and_unknown_fields() {
    let mut value: serde_json::Value =
        serde_json::from_slice(include_bytes!("fixtures/homebrew/info-cask.json")).unwrap();
    for installed in [serde_json::json!("1.0"), serde_json::json!(["1.0"])] {
        value["casks"][0]["installed"] = installed;
        value["future_field"] = serde_json::json!(true);
        assert_eq!(
            parser::info(&serde_json::to_vec(&value).unwrap()).unwrap()[0]
                .package
                .installed,
            ["1.0"]
        );
    }
}
