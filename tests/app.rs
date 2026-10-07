use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use lazybrew::{
    app::{App, Event, Tab},
    backend::{CommandEvent, cli::CliBackend, parser},
    ui,
};
use std::sync::Arc;
fn key(c: KeyCode) -> KeyEvent {
    KeyEvent::new(c, KeyModifiers::NONE)
}
#[test]
fn keyboard_confirmation_filter_and_busy_guard() {
    let backend = Arc::new(CliBackend::new("/missing/brew".into()));
    let (tx, _) = tokio::sync::mpsc::channel(256);
    let mut app = App::new(100);
    app.installed = parser::info(include_bytes!("fixtures/homebrew/installed.json"))
        .unwrap()
        .into_iter()
        .map(|i| i.package)
        .collect();
    app.key(key(KeyCode::Char('x')), backend.clone(), tx.clone());
    assert!(app.pending.is_some());
    app.key(key(KeyCode::Esc), backend.clone(), tx.clone());
    assert!(app.pending.is_none());
    app.key(key(KeyCode::Char('/')), backend.clone(), tx.clone());
    for c in "atuin".chars() {
        app.key(key(KeyCode::Char(c)), backend.clone(), tx.clone());
    }
    app.key(key(KeyCode::Enter), backend.clone(), tx.clone());
    assert_eq!(app.count(), 1);
    app.key(key(KeyCode::Right), backend.clone(), tx.clone());
    assert!(app.tab == Tab::Casks);
    assert!(app.filter.is_empty());
    app.busy = true;
    app.key(key(KeyCode::Char('q')), backend.clone(), tx.clone());
    assert!(!app.quit);
    app.key(key(KeyCode::Char('U')), backend.clone(), tx.clone());
    assert!(app.pending.is_none());
    app.busy = false;
    app.key(key(KeyCode::Char('q')), backend, tx);
    assert!(app.quit);
}
#[test]
fn bounded_logs_and_stale_detail_results() {
    let backend = Arc::new(CliBackend::new("/missing/brew".into()));
    let (tx, _) = tokio::sync::mpsc::channel(256);
    let mut app = App::new(100);
    app.event(
        Event::Output(CommandEvent::Output("line\n".repeat(1000))),
        backend.clone(),
        tx.clone(),
    );
    assert_eq!(app.output.len(), 100);
    app.event(Event::Details(999, Ok("stale".into())), backend, tx);
    assert_ne!(app.details, "stale");
}
#[test]
fn render_all_views_and_small_terminal() {
    use ratatui::{Terminal, backend::TestBackend};
    for (w, h) in [(120, 40), (55, 15), (20, 5)] {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        let mut app = App::new(100);
        for tab in Tab::ALL {
            app.tab = tab;
            terminal.draw(|f| ui::draw(f, &mut app)).unwrap();
        }
        app.help = true;
        terminal.draw(|f| ui::draw(f, &mut app)).unwrap();
    }
}

fn render(app: &mut App) {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 38)).unwrap();
    terminal.draw(|frame| ui::draw(frame, app)).unwrap();
}
fn click(app: &mut App, action: impl Fn(&lazybrew::app::HitAction) -> bool) {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let hit = app
        .hits
        .iter()
        .find(|hit| action(&hit.action))
        .expect("visible hit target");
    let event = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: hit.rect.x,
        row: hit.rect.y,
        modifiers: KeyModifiers::NONE,
    };
    let (tx, _) = tokio::sync::mpsc::channel(8);
    app.mouse(event, Arc::new(CliBackend::new("/missing/brew".into())), tx);
}
#[test]
fn mouse_switches_workspaces_and_opens_and_cancels_confirmation() {
    use lazybrew::app::HitAction;
    let mut app = App::new(100);
    app.services = parser::services(include_bytes!("fixtures/homebrew/services.json")).unwrap();
    render(&mut app);
    click(&mut app, |h| matches!(h, HitAction::Tab(Tab::Services)));
    assert_eq!(app.tab, Tab::Services);
    app.editing = true;
    app.filter = "atuin".into();
    render(&mut app);
    click(&mut app, |h| {
        matches!(h, HitAction::Key(KeyCode::Char('t')))
    });
    assert!(app.pending.is_some());
    render(&mut app);
    assert!(
        app.hits
            .iter()
            .all(|h| matches!(h.action, HitAction::Key(KeyCode::Char('y' | 'n'))))
    );
    click(&mut app, |h| {
        matches!(h, HitAction::Key(KeyCode::Char('n')))
    });
    assert!(app.pending.is_none());
    assert!(!app.busy);
}
#[test]
fn mouse_selects_correct_row_after_scrolling_and_wheel_targets_panel() {
    use crossterm::event::{MouseEvent, MouseEventKind};
    use lazybrew::{
        app::{Focus, HitAction},
        domain::{PackageId, PackageKind},
    };
    let mut app = App::new(100);
    let sample = parser::info(include_bytes!("fixtures/homebrew/installed.json")).unwrap()[0]
        .package
        .clone();
    for n in 0..100 {
        let mut p = sample.clone();
        p.id = PackageId::new(format!("package-{n}"), PackageKind::Formula).unwrap();
        app.installed.push(p);
    }
    app.selected = 70;
    render(&mut app);
    let offset = app.list_state.offset();
    assert!(offset > 0);
    click(&mut app, |h| matches!(h,HitAction::Select(i) if *i==offset));
    assert_eq!(app.selected, offset);
    render(&mut app);
    let hit = app
        .hits
        .iter()
        .find(|h| matches!(h.action, HitAction::Focus(Focus::Details)))
        .unwrap();
    let event = MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: hit.rect.x,
        row: hit.rect.y,
        modifiers: KeyModifiers::NONE,
    };
    let (tx, _) = tokio::sync::mpsc::channel(8);
    app.mouse(event, Arc::new(CliBackend::new("/missing/brew".into())), tx);
    assert_eq!(app.focus, Focus::Details);
    assert_eq!(app.detail_scroll, 3);
    assert_eq!(app.selected, offset);
}
#[test]
fn selection_preview_is_immediate_and_reuses_cached_details() {
    let mut app = App::new(100);
    app.installed = parser::info(include_bytes!("fixtures/homebrew/installed.json"))
        .unwrap()
        .into_iter()
        .map(|p| p.package)
        .collect();
    app.sync_preview();
    assert!(!app.details.is_empty());
    assert!(app.details_delay().is_some());
    let (tx, _) = tokio::sync::mpsc::channel(8);
    app.event(
        Event::Details(1, Ok("Cached details".into())),
        Arc::new(CliBackend::new("/missing/brew".into())),
        tx,
    );
    app.change_tab(Tab::Casks);
    app.change_tab(Tab::Formulae);
    assert_eq!(app.details, "Cached details");
    assert!(app.details_delay().is_none());
}
#[test]
fn tab_focus_and_page_scroll_leave_selected_package_unchanged() {
    use lazybrew::app::Focus;
    let mut app = App::new(100);
    let (tx, _) = tokio::sync::mpsc::channel(8);
    let backend = Arc::new(CliBackend::new("/missing/brew".into()));
    app.key(key(KeyCode::Tab), backend.clone(), tx.clone());
    assert_eq!(app.focus, Focus::Details);
    app.key(key(KeyCode::PageDown), backend, tx);
    assert_eq!(app.detail_scroll, 10);
    assert_eq!(app.selected, 0);
}
