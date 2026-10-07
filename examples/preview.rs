//! Render the actual interface with fictional data; never reads the local Homebrew installation.
use lazybrew::{
    app::{App, Focus, Tab},
    domain::{Package, PackageId, PackageKind, Service},
    ui,
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    style::{Color, Modifier},
};
use std::fmt::Write;
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn color(color: Color) -> String {
    match color {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::LightRed => "#ff8585".into(),
        _ => "#d6deeb".into(),
    }
}
fn main() -> anyhow::Result<()> {
    let mut app = App::new(100);
    let light = std::env::args().any(|arg| arg == "--light");
    if light {
        app.theme = lazybrew::theme::Theme::Light;
    }
    for (name, version) in [
        ("mysql@9.7", "9.7.2"),
        ("postgresql@18", "18.6"),
        ("redis", "8.2.1"),
        ("node", "24.0.0"),
        ("git", "2.50.0"),
        ("jq", "1.8.0"),
    ] {
        app.installed.push(Package {
            id: PackageId::new(name, PackageKind::Formula)?,
            installed: vec![version.into()],
            version: version.into(),
            description: String::new(),
            pinned: false,
            outdated: false,
        });
    }
    for (name, status, pid) in [
        ("mysql@9.7", "started", Some(2048)),
        ("postgresql@18", "started", Some(4096)),
        ("redis", "none", None),
    ] {
        app.services.push(Service {
            name: name.into(),
            status: status.into(),
            user: Some("demo".into()),
            file: None,
            exit_code: None,
            running: Some(pid.is_some()),
            loaded: Some(pid.is_some()),
            schedulable: Some(false),
            pid,
        });
    }
    app.change_tab(Tab::Services);
    app.selected = 1;
    app.sync_preview();
    app.detail_loading = false;
    app.focus = Focus::Packages;
    app.details="Object-relational database system\n\nSTATUS\nStarted · ready for connections\n\nPROCESS\n4096\n\nRUNNING / LOADED\nYes / Yes\n\nUSER\ndemo\n\nNOTES\nManaged by Homebrew. Starts now and at login.\nUse Stop to stop the service and disable login startup.".into();
    app.output.extend(
        [
            "› Start service postgresql@18 (formula)",
            "Successfully started postgresql@18",
            "Completed · exit 0",
            "",
            "› Start service mysql@9.7 (formula)",
            "Successfully started mysql@9.7",
            "Completed · exit 0",
        ]
        .into_iter()
        .map(String::from),
    );
    app.status = "All caught up · 2 services running · example workspace".into();
    let mut terminal = Terminal::new(TestBackend::new(120, 38))?;
    terminal.draw(|f| ui::draw(f, &mut app))?;
    let buffer = terminal.backend().buffer();
    let mut svg = String::from(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1240\" height=\"814\" viewBox=\"0 0 1240 814\" role=\"img\" aria-label=\"LazyBrew services workspace, rendered with fictional demo data\">\n<rect width=\"1240\" height=\"814\" rx=\"16\" fill=\"#12161e\"/><circle cx=\"26\" cy=\"23\" r=\"6\" fill=\"#ef8585\"/><circle cx=\"46\" cy=\"23\" r=\"6\" fill=\"#efb689\"/><circle cx=\"66\" cy=\"23\" r=\"6\" fill=\"#82dab9\"/><text x=\"620\" y=\"28\" fill=\"#77869c\" font-size=\"13\" text-anchor=\"middle\" font-family=\"monospace\">lazybrew — services</text>\n",
    );
    svg = svg
        .replace("#12161e", &color(app.theme.palette().bg))
        .replace("#77869c", &color(app.theme.palette().muted));
    for y in 0..38 {
        for x in 0..120 {
            let cell = &buffer[(x, y)];
            let px = 20 + x * 10;
            let py = 46 + y * 20;
            writeln!(
                svg,
                "<rect x=\"{px}\" y=\"{py}\" width=\"10\" height=\"20\" fill=\"{}\"/>",
                color(cell.bg)
            )?;
        }
    }
    svg.push_str(
        "<g font-family=\"Menlo,Consolas,monospace\" font-size=\"15\" xml:space=\"preserve\">\n",
    );
    for y in 0..38 {
        for x in 0..120 {
            let cell = &buffer[(x, y)];
            if cell.symbol().trim().is_empty() {
                continue;
            }
            writeln!(
                svg,
                "<text x=\"{}\" y=\"{}\" fill=\"{}\" font-weight=\"{}\" textLength=\"10\" lengthAdjust=\"spacingAndGlyphs\">{}</text>",
                20 + x * 10,
                61 + y * 20,
                color(cell.fg),
                if cell.modifier.contains(Modifier::BOLD) {
                    "bold"
                } else {
                    "normal"
                },
                escape(cell.symbol())
            )?;
        }
    }
    svg.push_str("</g></svg>\n");
    std::fs::create_dir_all("docs")?;
    std::fs::write(
        if light {
            "docs/preview-light.svg"
        } else {
            "docs/preview.svg"
        },
        svg,
    )?;
    Ok(())
}
