#![cfg(unix)]
use lazybrew::backend::{CommandEvent, runner};
use std::{
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
// Linux can return ETXTBSY if another concurrent fork inherits a newly written
// fixture's writable descriptor before exec closes it. Serialize fixture tests;
// each test still exercises the runner's concurrent stdout/stderr and cancellation.
static FIXTURE_GATE: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
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
    let _fixture_guard = FIXTURE_GATE.acquire().await.unwrap();
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
            CommandEvent::Cancelled => panic!("unexpected cancellation"),
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
    let _fixture_guard = FIXTURE_GATE.acquire().await.unwrap();
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
    let _fixture_guard = FIXTURE_GATE.acquire().await.unwrap();
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

#[tokio::test]
async fn cancellation_terminates_descendants_and_emits_one_terminal_event() {
    let _fixture_guard = FIXTURE_GATE.acquire().await.unwrap();
    let fake = Fake::new(
        "import subprocess,time\np=subprocess.Popen(['sleep','30'])\nprint(p.pid, flush=True)\ntime.sleep(30)",
    );
    let mut handle = runner::spawn(&fake.0, &[]).unwrap();
    let first = tokio::time::timeout(Duration::from_secs(3), handle.events.recv())
        .await
        .unwrap()
        .unwrap();
    let pid: i32 = match first {
        CommandEvent::Output(text) => text.trim().parse().unwrap(),
        _ => panic!("expected pid"),
    };
    handle.cancel();
    let mut cancelled = 0;
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(event) = handle.events.recv().await {
            match event {
                CommandEvent::Cancelled => cancelled += 1,
                CommandEvent::Finished { .. } => panic!("cancelled command finished normally"),
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(cancelled, 1);
    // A killed descendant may briefly be a zombie on Linux; it must not be running.
    let result = std::process::Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    let state = String::from_utf8_lossy(&result.stdout);
    assert!(
        state.trim().is_empty() || state.trim().starts_with('Z'),
        "descendant still running: {state}"
    );
}

#[tokio::test]
async fn plans_stop_on_failure_and_emit_a_single_result() {
    let _fixture_guard = FIXTURE_GATE.acquire().await.unwrap();
    let fake = Fake::new(
        "import sys\nprint('executed '+sys.argv[1],flush=True)\nsys.exit(9 if sys.argv[1]=='fail' else 0)",
    );
    let mut handle = runner::spawn_plan(
        &fake.0,
        vec![
            vec!["first".into()],
            vec!["fail".into()],
            vec!["last".into()],
        ],
    )
    .unwrap();
    let mut output = String::new();
    let mut terminal = 0;
    while let Some(event) = handle.events.recv().await {
        match event {
            CommandEvent::Output(text) => output.push_str(&text),
            CommandEvent::Finished { success, code } => {
                assert!(!success);
                assert_eq!(code, Some(9));
                terminal += 1;
            }
            _ => panic!("unexpected cancellation"),
        }
    }
    assert!(output.contains("executed first") && output.contains("executed fail"));
    assert!(!output.contains("last"));
    assert_eq!(terminal, 1);
}
#[tokio::test]
async fn cancelled_plan_never_starts_later_steps() {
    let _fixture_guard = FIXTURE_GATE.acquire().await.unwrap();
    let fake =
        Fake::new("import sys,time\nprint('executed '+sys.argv[1],flush=True)\ntime.sleep(30)");
    let mut handle =
        runner::spawn_plan(&fake.0, vec![vec!["first".into()], vec!["later".into()]]).unwrap();
    let mut output = String::new();
    while !output.contains("executed first") {
        if let Some(CommandEvent::Output(text)) =
            tokio::time::timeout(Duration::from_secs(3), handle.events.recv())
                .await
                .unwrap()
        {
            output.push_str(&text);
        }
    }
    handle.cancel();
    let mut terminal = 0;
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(event) = handle.events.recv().await {
            match event {
                CommandEvent::Cancelled => terminal += 1,
                CommandEvent::Output(text) => output.push_str(&text),
                _ => panic!("unexpected normal completion"),
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(terminal, 1);
    assert!(!output.contains("later"));
}
