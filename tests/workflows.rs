use lazybrew::{
    backend::cli::operation_plan,
    domain::*,
    workflows::{Stack, switch_version},
};
fn id(name: &str) -> PackageId {
    PackageId::new(name, PackageKind::Formula).unwrap()
}
fn package(name: &str) -> Package {
    Package {
        id: id(name),
        installed: vec!["1".into()],
        version: "1".into(),
        description: String::new(),
        pinned: false,
        outdated: false,
    }
}
#[test]
fn runtime_switch_orders_service_stop_unlinks_link_and_start() {
    for family in ["php", "node", "postgresql"] {
        let old = format!("{family}@1");
        let target = format!("{family}@2");
        let installed = vec![package(&old), package(&target), package("unrelated")];
        let service = |name: String, status: &str| Service {
            name,
            status: status.into(),
            user: None,
            file: None,
            exit_code: None,
            running: None,
            loaded: None,
            schedulable: None,
            pid: None,
        };
        let services = vec![
            service(old.clone(), "started"),
            service(target.clone(), "none"),
        ];
        let plan =
            operation_plan(&switch_version(&id(&target), &installed, &services).unwrap()).unwrap();
        assert_eq!(
            plan,
            vec![
                vec!["services", "stop", &old],
                vec!["unlink", "--formula", &old],
                vec!["link", "--force", "--formula", &target],
                vec!["services", "start", &target]
            ]
        );
        assert!(!plan.iter().flatten().any(|arg| arg == "--overwrite"));
    }
    assert!(switch_version(&id("redis"), &[package("redis")], &[]).is_err());
    assert!(switch_version(&id("node@2"), &[package("node@1")], &[]).is_err());
}
#[test]
fn stack_setup_installs_missing_before_services_and_validates_members() {
    let stack = Stack {
        name: "Web".into(),
        formulae: vec!["node".into(), "postgresql@18".into()],
        services: vec!["postgresql@18".into()],
    };
    let plan = operation_plan(&stack.plan('i', &[package("node")]).unwrap()).unwrap();
    assert_eq!(
        plan,
        vec![
            vec!["install", "--formula", "postgresql@18"],
            vec!["services", "start", "postgresql@18"]
        ]
    );
    assert_eq!(
        operation_plan(&stack.plan('t', &[]).unwrap()).unwrap(),
        vec![vec!["services", "stop", "postgresql@18"]]
    );
    assert_eq!(
        operation_plan(&stack.plan('s', &[]).unwrap()).unwrap(),
        vec![vec!["services", "start", "postgresql@18"]]
    );
    let invalid = Stack {
        services: vec!["mysql".into()],
        ..stack.clone()
    };
    assert!(invalid.validate().is_err());
    let invalid = Stack {
        formulae: vec!["--force".into()],
        services: vec![],
        ..stack
    };
    assert!(invalid.validate().is_err());
}
