use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
};
use lazybrew::{
    app::{App, Event as AppEvent},
    backend::cli::CliBackend,
    config::Config,
    ui,
};
use std::{
    io::{self, IsTerminal},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
#[derive(Parser)]
#[command(version, about = "A keyboard-first Homebrew dashboard")]
struct Args {
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    brew_path: Option<PathBuf>,
    #[arg(long)]
    data_dir: Option<PathBuf>,
    #[arg(long, value_enum)]
    theme: Option<lazybrew::theme::ThemeChoice>,
}
struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), DisableMouseCapture);
        ratatui::restore();
    }
}
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let mut config = Config::load(args.config)?;
    if let Some(path) = args.brew_path {
        config.brew_path = path;
    }
    anyhow::ensure!(
        io::stdin().is_terminal() && io::stdout().is_terminal(),
        "LazyBrew needs an interactive terminal"
    );
    let data_dir = args
        .data_dir
        .unwrap_or(lazybrew::config::dirs()?.data_local_dir().to_path_buf());
    let log_dir = data_dir.join("logs");
    std::fs::create_dir_all(&log_dir)?;
    let (writer, _log_guard) =
        tracing_appender::non_blocking(tracing_appender::rolling::daily(log_dir, "lazybrew.log"));
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "lazybrew=info".into()),
        )
        .with_writer(writer)
        .with_ansi(false)
        .init();
    let backend = Arc::new(CliBackend::new(config.brew_path.clone()));
    let (tx, mut rx) = tokio::sync::mpsc::channel(256);
    let mut app = App::new(config.max_output_lines);
    app.theme = args.theme.unwrap_or(config.theme).resolve();
    app.stacks = config.stacks;
    app.configure(
        config.refresh_interval_secs,
        lazybrew::storage::Store::new(data_dir.join("state.json"), config.brew_path),
    );
    let mut terminal = ratatui::init();
    let _guard = TerminalGuard;
    execute!(io::stdout(), EnableMouseCapture)?;
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(io::stdout(), DisableMouseCapture);
        previous_hook(info);
    }));
    // Blocking terminal reads live off the async executor. Input wakes the UI immediately.
    let input_tx = tx.clone();
    std::thread::spawn(move || {
        loop {
            match event::read() {
                Ok(event) => {
                    if input_tx.blocking_send(AppEvent::Terminal(event)).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    let _ = input_tx.blocking_send(AppEvent::InputError(error.to_string()));
                    break;
                }
            }
        }
    });
    app.refresh(backend.clone(), tx.clone());
    while !app.quit {
        terminal.draw(|f| ui::draw(f, &mut app))?;
        let delay = app.details_delay();
        let refresh_delay = app.refresh_delay();
        tokio::select! {
            message = rx.recv() => {
                let Some(message) = message else { break; };
                dispatch(message, &mut app, &backend, &tx);
                // Coalesce bursts of input/output into one frame without starving input.
                for _ in 0..63 {
                    match rx.try_recv() { Ok(message) => dispatch(message, &mut app, &backend, &tx), Err(_) => break }
                }
            }
            _ = tokio::time::sleep(refresh_delay.unwrap_or(Duration::from_secs(3600))), if refresh_delay.is_some() => {
                app.event(AppEvent::Tick, backend.clone(), tx.clone());
            }
            _ = tokio::time::sleep(delay.unwrap_or(Duration::from_secs(3600))), if delay.is_some() => {
                app.fetch_details(backend.clone(), tx.clone());
            }
        }
    }
    Ok(())
}
fn dispatch(
    message: AppEvent,
    app: &mut App,
    backend: &Arc<CliBackend>,
    tx: &tokio::sync::mpsc::Sender<AppEvent>,
) {
    match message {
        AppEvent::Terminal(Event::Key(key)) if key.kind != KeyEventKind::Release => {
            app.key(key, backend.clone(), tx.clone())
        }
        AppEvent::Terminal(Event::Mouse(mouse)) => app.mouse(mouse, backend.clone(), tx.clone()),
        AppEvent::Terminal(_) => {}
        event => app.event(event, backend.clone(), tx.clone()),
    }
}
