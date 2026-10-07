#![cfg(unix)]
use lazybrew::backend::{CommandEvent, runner};
use std::{
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fake(PathBuf);
impl Fake {
    fn new(body: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "lazybrew-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, format!("#!/usr/bin/env python3\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
}
impl Drop for Fake {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
#[tokio::test]
async fn output_is_streamed_before_exit_and_stderr_is_drained() {
    let fake = Fake::new(
        "import sys,time\nprint('first', flush=True)\ntime.sleep(0.3)\nprint('diagnostic', file=sys.stderr, flush=True)\nsys.exit(7)",
    );
    let mut handle = runner::spawn(&fake.0, &[]).unwrap();
    let first = tokio::time::timeout(Duration::from_secs(2), handle.events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(first, CommandEvent::Output(ref text) if text.contains("first")));
    let mut stderr = false;
    let mut failed = false;
    while let Some(e) = handle.events.recv().await {
        match e {
            CommandEvent::Output(text) => stderr |= text.contains("diagnostic"),
            CommandEvent::Finished { success, code } => {
                assert!(!success);
                assert_eq!(code, Some(7));
                failed = true;
            }
        }
    }
    assert!(stderr && failed);
}
#[tokio::test]
async fn query_preserves_arguments_and_reports_errors() {
    let fake = Fake::new("import json,sys\nprint(json.dumps(sys.argv[1:]))");
    let args = vec!["one argument".into(), "$(touch /tmp/never)".into()];
    let bytes = runner::query(&fake.0, &args).await.unwrap();
    assert_eq!(serde_json::from_slice::<Vec<String>>(&bytes).unwrap(), args);
    let fake = Fake::new("import sys\nprint('useful error', file=sys.stderr)\nsys.exit(1)");
    assert!(
        runner::query(&fake.0, &[])
            .await
            .unwrap_err()
            .to_string()
            .contains("useful error")
    );
}

#[tokio::test]
async fn search_retains_casks_when_formula_query_has_no_matches() {
    use lazybrew::{
        backend::{BrewBackend, cli::CliBackend},
        domain::PackageKind,
    };
    let fake = Fake::new(
        "import sys\nif '--formula' in sys.argv:\n print('Error: No formulae or casks found for \\\"test\\\".', file=sys.stderr)\n sys.exit(1)\nprint('firefox')",
    );
    let backend = CliBackend::new(fake.0.clone());
    let packages = backend.search("firefox").await.unwrap();
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].id.kind, PackageKind::Cask);
}
