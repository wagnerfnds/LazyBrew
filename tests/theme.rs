use lazybrew::{
    app::App,
    theme::{Theme, ThemeChoice, colorfgbg_theme},
    ui,
};
use ratatui::{Terminal, backend::TestBackend, style::Color};
#[test]
fn theme_settings_and_environment_fallbacks() {
    assert_eq!(ThemeChoice::Light.resolve(), Theme::Light);
    assert_eq!(ThemeChoice::Dark.resolve(), Theme::Dark);
    for (value, expected) in [
        ("0;15", Some(Theme::Light)),
        ("15;0", Some(Theme::Dark)),
        ("0;default;7", Some(Theme::Light)),
        ("0;231", Some(Theme::Light)),
        ("0;232", Some(Theme::Dark)),
        ("0;255", Some(Theme::Light)),
        ("invalid", None),
        ("0;999", None),
    ] {
        assert_eq!(colorfgbg_theme(Some(value)), expected);
    }
    assert_eq!(colorfgbg_theme(None), None);
    assert_eq!(
        toml::from_str::<lazybrew::config::Config>("theme = 'light'")
            .unwrap()
            .theme,
        ThemeChoice::Light
    );
    assert!(toml::from_str::<lazybrew::config::Config>("theme = 'unknown'").is_err());
}
fn luminance(color: Color) -> f64 {
    let Color::Rgb(r, g, b) = color else {
        panic!("RGB palette required")
    };
    let linear = |v: u8| {
        let v = v as f64 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}
#[test]
fn both_palettes_keep_text_and_status_legible() {
    for theme in [Theme::Light, Theme::Dark] {
        let p = theme.palette();
        for bg in [p.bg, p.panel, p.selected] {
            for fg in [p.text, p.muted, p.accent, p.warning, p.error] {
                let (fg, bg) = (luminance(fg), luminance(bg));
                let contrast = (fg.max(bg) + 0.05) / (fg.min(bg) + 0.05);
                assert!(contrast >= 4.5, "{theme:?}: contrast {contrast}");
            }
        }
    }
}
#[test]
fn light_palette_reaches_all_workspaces_and_overlays() {
    let mut app = App::new(100);
    app.theme = Theme::Light;
    let mut terminal = Terminal::new(TestBackend::new(120, 38)).unwrap();
    for tab in lazybrew::app::Tab::ALL {
        app.tab = tab;
        terminal.draw(|f| ui::draw(f, &mut app)).unwrap();
        assert_eq!(
            terminal.backend().buffer()[(0, 0)].bg,
            app.theme.palette().bg
        );
    }
    app.help = true;
    terminal.draw(|f| ui::draw(f, &mut app)).unwrap();
    app.help = false;
    app.history_visible = true;
    terminal.draw(|f| ui::draw(f, &mut app)).unwrap();
    app.history_visible = false;
    app.stacks_visible = true;
    terminal.draw(|f| ui::draw(f, &mut app)).unwrap();
    app.stacks_visible = false;
    app.pending = Some(lazybrew::domain::Operation::Update);
    terminal.draw(|f| ui::draw(f, &mut app)).unwrap();
    assert!(
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .any(|cell| cell.bg == app.theme.palette().panel)
    );
}
