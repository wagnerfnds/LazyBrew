use lazybrew::{
    domain::{PackageId, PackageKind},
    storage::{HistoryEntry, Snapshot, Store, now},
};
#[test]
fn state_survives_restart_is_scoped_to_backend_and_marks_interruption() {
    let dir = std::env::temp_dir().join(format!("lazybrew-storage-{}", std::process::id()));
    let path = dir.join("state.json");
    let store = Store::new(path.clone(), "fake-brew".into());
    let mut snapshot = Snapshot {
        updated: now(),
        ..Snapshot::default()
    };
    snapshot.details.push_back((
        (
            false,
            PackageId::new("node@22", PackageKind::Formula).unwrap(),
        ),
        "details".into(),
    ));
    snapshot.history.push_back(HistoryEntry {
        operation: "install".into(),
        started: now(),
        finished: None,
        result: "Running".into(),
    });
    store.save(snapshot).unwrap();
    let loaded = store.load().unwrap();
    assert_eq!(loaded.details.len(), 1);
    assert!(loaded.history[0].result.starts_with("Interrupted"));
    assert!(loaded.history[0].finished.is_some());
    assert!(
        Store::new(path.clone(), "other-brew".into())
            .load()
            .unwrap()
            .details
            .is_empty()
    );
    let stale = Snapshot {
        updated: 1,
        details: loaded.details,
        ..Snapshot::default()
    };
    store.save(stale).unwrap();
    assert!(store.load().unwrap().details.is_empty());
    std::fs::write(&path, "{broken").unwrap();
    assert!(store.load().is_err());
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn saved_identifiers_are_revalidated() {
    assert!(serde_json::from_str::<PackageId>(r#"{"name":"--force","kind":"Formula"}"#).is_err());
}
