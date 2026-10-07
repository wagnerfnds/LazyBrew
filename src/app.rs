use crate::{
    backend::{BrewBackend, CommandEvent},
    domain::*,
    storage::{self, HistoryEntry, Snapshot, Store},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    layout::{Position, Rect},
    widgets::ListState,
};
use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::mpsc, task::JoinHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Formulae,
    Casks,
    Available,
    Outdated,
    Services,
}
impl Tab {
    pub const ALL: [Self; 5] = [
        Self::Formulae,
        Self::Casks,
        Self::Available,
        Self::Outdated,
        Self::Services,
    ];
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|t| *t == self).unwrap()
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Formulae => "Formulae",
            Self::Casks => "Casks",
            Self::Available => "Discover",
            Self::Outdated => "Updates",
            Self::Services => "Services",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Packages,
    Details,
    Output,
}
#[derive(Clone, Copy)]
pub enum HitAction {
    Tab(Tab),
    Select(usize),
    Focus(Focus),
    Key(KeyCode),
}
#[derive(Clone, Copy)]
pub struct Hit {
    pub rect: Rect,
    pub action: HitAction,
}
pub enum Event {
    Installed(Result<Vec<Package>, String>),
    Outdated(Result<Vec<Package>, String>),
    Services(Result<Vec<Service>, String>),
    Catalogue(Result<Vec<Package>, String>),
    Search(u64, Result<Vec<Package>, String>),
    Details(u64, Result<String, String>),
    Output(CommandEvent),
    Error(String),
    Terminal(crossterm::event::Event),
    InputError(String),
    Tick,
}
pub struct App {
    pub tab: Tab,
    pub theme: crate::theme::Theme,
    pub focus: Focus,
    pub selected: usize,
    pub list_state: ListState,
    pub installed: Vec<Package>,
    pub outdated: Vec<Package>,
    pub services: Vec<Service>,
    pub available: Vec<Package>,
    catalogue: Vec<Package>,
    pub filter: String,
    pub editing: bool,
    pub details: String,
    pub output: VecDeque<String>,
    pub status: String,
    pub pending: Option<Operation>,
    pub pending_scroll: u16,
    search_results: bool,
    pub busy: bool,
    pub help: bool,
    pub stacks_visible: bool,
    pub stacks: Vec<crate::workflows::Stack>,
    pub stack_selected: usize,
    pub history_visible: bool,
    pub history_scroll: u16,
    pub history: VecDeque<HistoryEntry>,
    store: Option<Store>,
    snapshot_updated: u64,
    refresh_interval: Duration,
    refresh_due: Instant,
    cancellation: Option<tokio::sync::oneshot::Sender<()>>,
    pub quit: bool,
    pub output_scroll: usize,
    pub detail_scroll: u16,
    pub hits: Vec<Hit>,
    pub loading: usize,
    pub detail_loading: bool,
    pub search_loading: bool,
    refresh_pending: bool,
    load_failed: bool,
    generation: u64,
    search_generation: u64,
    max_lines: usize,
    preview_key: Option<(bool, PackageId)>,
    detail_due: Option<Instant>,
    detail_task: Option<JoinHandle<()>>,
    cache: VecDeque<((bool, PackageId), String)>,
}
impl App {
    pub fn new(max_lines: usize) -> Self {
        Self {
            tab: Tab::Formulae,
            theme: crate::theme::Theme::Dark,
            focus: Focus::Packages,
            selected: 0,
            list_state: ListState::default(),
            installed: vec![],
            outdated: vec![],
            services: vec![],
            available: vec![],
            catalogue: vec![],
            filter: String::new(),
            editing: false,
            details: String::new(),
            output: VecDeque::new(),
            status: "Connecting to Homebrew…".into(),
            pending: None,
            pending_scroll: 0,
            search_results: false,
            busy: false,
            help: false,
            stacks_visible: false,
            stacks: Vec::new(),
            stack_selected: 0,
            history_visible: false,
            history_scroll: 0,
            history: VecDeque::new(),
            store: None,
            snapshot_updated: 0,
            refresh_interval: Duration::from_secs(300),
            refresh_due: Instant::now() + Duration::from_secs(300),
            cancellation: None,
            quit: false,
            output_scroll: 0,
            detail_scroll: 0,
            hits: vec![],
            loading: 0,
            detail_loading: false,
            search_loading: false,
            refresh_pending: false,
            load_failed: false,
            generation: 0,
            search_generation: 0,
            max_lines,
            preview_key: None,
            detail_due: None,
            detail_task: None,
            cache: VecDeque::new(),
        }
    }
    pub fn configure(&mut self, interval_secs: u64, store: Store) {
        self.refresh_interval = Duration::from_secs(interval_secs);
        self.refresh_due = Instant::now() + self.refresh_interval;
        match store.load() {
            Ok(snapshot) => {
                self.installed = snapshot.installed;
                self.outdated = snapshot.outdated;
                self.services = snapshot.services;
                self.catalogue = snapshot.catalogue;
                self.available = self.catalogue.clone();
                self.cache = snapshot.details;
                self.history = snapshot.history;
                self.snapshot_updated = snapshot.updated;
                if snapshot.updated > 0 {
                    self.status =
                        format!("Cached snapshot from {} · refreshing…", snapshot.updated);
                }
            }
            Err(e) => self.log(&format!("Could not load saved state: {e}")),
        }
        self.store = Some(store);
        self.sync_preview();
    }
    fn persist(&mut self) {
        if let Some(store) = &self.store {
            let result = store.save(Snapshot {
                updated: self.snapshot_updated,
                installed: self.installed.clone(),
                outdated: self.outdated.clone(),
                services: self.services.clone(),
                catalogue: self.catalogue.clone(),
                details: self.cache.clone(),
                history: self.history.clone(),
            });
            if let Err(e) = result {
                self.log(&format!("Could not save state: {e}"));
            }
        }
    }
    fn finish_operation(&mut self, result: String) {
        self.busy = false;
        self.cancellation = None;
        if let Some(entry) = self
            .history
            .front_mut()
            .filter(|entry| entry.finished.is_none())
        {
            entry.finished = Some(storage::now());
            entry.result = result;
        }
        self.persist();
    }
    pub fn refresh_delay(&self) -> Option<Duration> {
        (self.refresh_interval != Duration::ZERO)
            .then(|| self.refresh_due.saturating_duration_since(Instant::now()))
    }
    pub fn packages(&self) -> Vec<&Package> {
        let source = match self.tab {
            Tab::Available => &self.available,
            Tab::Outdated => &self.outdated,
            _ => &self.installed,
        };
        let filter = self.filter.to_lowercase();
        source
            .iter()
            .filter(|p| match self.tab {
                Tab::Formulae => p.id.kind == PackageKind::Formula,
                Tab::Casks => p.id.kind == PackageKind::Cask,
                _ => true,
            })
            .filter(|p| {
                filter.is_empty()
                    || p.id.name().to_lowercase().contains(&filter)
                    || p.description.to_lowercase().contains(&filter)
            })
            .collect()
    }
    pub fn service_rows(&self) -> Vec<&Service> {
        let filter = self.filter.to_lowercase();
        self.services
            .iter()
            .filter(|s| filter.is_empty() || s.name.to_lowercase().contains(&filter))
            .collect()
    }
    pub fn count(&self) -> usize {
        if self.tab == Tab::Services {
            self.service_rows().len()
        } else {
            self.packages().len()
        }
    }
    pub fn id(&self) -> Option<PackageId> {
        if self.tab == Tab::Services {
            self.service_rows()
                .get(self.selected)
                .and_then(|s| PackageId::new(&s.name, PackageKind::Formula).ok())
        } else {
            self.packages().get(self.selected).map(|p| p.id.clone())
        }
    }
    pub fn tab_count(&self, tab: Tab) -> usize {
        match tab {
            Tab::Formulae => self
                .installed
                .iter()
                .filter(|p| p.id.kind == PackageKind::Formula)
                .count(),
            Tab::Casks => self
                .installed
                .iter()
                .filter(|p| p.id.kind == PackageKind::Cask)
                .count(),
            Tab::Available => self.available.len(),
            Tab::Outdated => self.outdated.len(),
            Tab::Services => self.services.len(),
        }
    }
    pub fn change_tab(&mut self, tab: Tab) {
        if self.tab == tab {
            self.focus = Focus::Packages;
            return;
        }
        self.tab = tab;
        self.selected = 0;
        self.list_state = ListState::default();
        self.filter.clear();
        self.editing = false;
        self.focus = Focus::Packages;
        self.search_generation += 1;
        self.search_loading = false;
        self.sync_preview();
    }
    /// Immediate local preview, then one debounced, cancellable lookup. Cache is bounded.
    pub fn sync_preview(&mut self) {
        self.selected = self.selected.min(self.count().saturating_sub(1));
        let key = self.id().map(|id| (self.tab == Tab::Services, id));
        if self.preview_key == key {
            return;
        }
        self.generation += 1;
        if let Some(task) = self.detail_task.take() {
            task.abort();
        }
        self.detail_due = None;
        self.detail_loading = false;
        self.detail_scroll = 0;
        self.preview_key = key.clone();
        self.details.clear();
        if let Some(key) = key {
            if let Some((_, text)) = self.cache.iter().find(|(k, _)| *k == key) {
                self.details = text.clone();
                return;
            }
            self.details = if key.0 {
                self.service_rows().get(self.selected).map(|s| format!("STATUS\n{}\n\nSERVICE\n{}\n\nSelect Start, Stop or Restart below.\nDetails will appear here automatically.", s.status, s.name)).unwrap_or_default()
            } else {
                self.packages()
                    .get(self.selected)
                    .map(|p| {
                        format!(
                            "{}\n\nINSTALLED\n{}\n\nLATEST\n{}",
                            p.description,
                            if p.installed.is_empty() {
                                "Not installed".into()
                            } else {
                                p.installed.join(", ")
                            },
                            if p.version.is_empty() {
                                "Checking…"
                            } else {
                                &p.version
                            }
                        )
                    })
                    .unwrap_or_default()
            };
            self.detail_loading = true;
            self.detail_due = Some(Instant::now() + Duration::from_millis(120));
        }
    }
    pub fn details_delay(&self) -> Option<Duration> {
        self.detail_due
            .map(|t| t.saturating_duration_since(Instant::now()))
    }
    pub fn fetch_details<B: BrewBackend>(&mut self, backend: Arc<B>, tx: mpsc::Sender<Event>) {
        self.detail_due = None;
        let Some((service, id)) = self.preview_key.clone() else {
            return;
        };
        if let Some(task) = self.detail_task.take() {
            task.abort();
        }
        let generation = self.generation;
        self.detail_task = Some(tokio::spawn(async move {
            let result = if service {
                backend.service_info(id.name()).await.map(|items| items.iter().map(|s| format!("STATUS\n{}\n\nPROCESS\n{}\n\nRUNNING / LOADED\n{} / {}\n\nUSER\n{}\n\nSERVICE FILE\n{}\n\nLAST EXIT CODE\n{}", s.status, s.pid.map(|p| p.to_string()).unwrap_or_else(|| "—".into()), s.running.map(|v| if v { "Yes" } else { "No" }).unwrap_or("Unknown"), s.loaded.map(|v| if v { "Yes" } else { "No" }).unwrap_or("Unknown"), s.user.as_deref().unwrap_or("—"), s.file.as_deref().unwrap_or("—"), s.exit_code.map(|v| v.to_string()).unwrap_or_else(|| "—".into()))).collect::<Vec<_>>().join("\n\n"))
            } else {
                backend.package_info(&id).await.map(|i| format!("{}\n\nINSTALLED\n{}\n\nLATEST\n{}{}\n\nHOMEPAGE\n{}\n\nDEPENDENCIES\n{}\n\nNOTES\n{}", i.package.description, if i.package.installed.is_empty() { "Not installed".into() } else { i.package.installed.join(", ") }, i.package.version, if i.package.pinned { "  ·  pinned" } else { "" }, i.homepage, if i.dependencies.is_empty() { "None".into() } else { i.dependencies.join(" · ") }, if i.caveats.is_empty() { "No additional setup required.".into() } else { i.caveats }))
            };
            let _ = tx
                .send(Event::Details(
                    generation,
                    result.map_err(|e| e.to_string()),
                ))
                .await;
        }));
    }
    pub fn refresh<B: BrewBackend>(&mut self, backend: Arc<B>, tx: mpsc::Sender<Event>) {
        self.refresh_due = Instant::now() + self.refresh_interval;
        if self.busy {
            self.refresh_pending = true;
            return;
        }
        if self.loading > 0 {
            self.refresh_pending = true;
            return;
        }
        self.loading = 4;
        self.load_failed = false;
        self.cache.clear();
        self.generation += 1;
        self.preview_key = None;
        if let Some(task) = self.detail_task.take() {
            task.abort();
        }
        self.status = "Refreshing Homebrew…".into();
        let b = backend.clone();
        let t = tx.clone();
        tokio::spawn(async move {
            let r = b.installed_packages().await.map_err(|e| e.to_string());
            let _ = t.send(Event::Installed(r)).await;
        });
        let b = backend.clone();
        let t = tx.clone();
        tokio::spawn(async move {
            let r = b.outdated_packages().await.map_err(|e| e.to_string());
            let _ = t.send(Event::Outdated(r)).await;
        });
        let b = backend.clone();
        let t = tx.clone();
        tokio::spawn(async move {
            let r = b.catalogue().await.map_err(|e| e.to_string());
            let _ = t.send(Event::Catalogue(r)).await;
        });
        tokio::spawn(async move {
            let r = backend.services().await.map_err(|e| e.to_string());
            let _ = tx.send(Event::Services(r)).await;
        });
    }
    fn log(&mut self, value: &str) {
        for line in value.replace('\r', "\n").lines() {
            self.output.push_back(
                line.chars()
                    .filter(|c| !c.is_control() || *c == '\t')
                    .take(4096)
                    .collect(),
            );
            while self.output.len() > self.max_lines {
                self.output.pop_front();
            }
        }
    }
    fn query_error(&mut self, error: String) {
        self.load_failed = true;
        self.status = error.clone();
        self.log(&error);
    }
    pub fn event<B: BrewBackend>(
        &mut self,
        event: Event,
        backend: Arc<B>,
        tx: mpsc::Sender<Event>,
    ) {
        let inventory_result = matches!(
            &event,
            Event::Installed(_) | Event::Outdated(_) | Event::Services(_) | Event::Catalogue(_)
        );
        match event {
            Event::Tick => {
                self.refresh_due = Instant::now() + self.refresh_interval;
                if !self.busy && self.loading == 0 && self.pending.is_none() && !self.editing {
                    self.refresh(backend.clone(), tx.clone());
                }
            }
            Event::Installed(r) => {
                self.loading = self.loading.saturating_sub(1);
                match r {
                    Ok(p) => self.installed = p,
                    Err(e) => self.query_error(e),
                }
            }
            Event::Outdated(r) => {
                self.loading = self.loading.saturating_sub(1);
                match r {
                    Ok(p) => self.outdated = p,
                    Err(e) => self.query_error(e),
                }
            }
            Event::Catalogue(r) => {
                self.loading = self.loading.saturating_sub(1);
                match r {
                    Ok(packages) => {
                        self.catalogue = packages;
                        if !self.search_loading && !self.search_results {
                            self.available = self.catalogue.clone();
                        }
                    }
                    Err(error) => self.query_error(error),
                }
            }
            Event::Services(r) => {
                self.loading = self.loading.saturating_sub(1);
                match r {
                    Ok(p) => self.services = p,
                    Err(e) => self.query_error(e),
                }
            }
            Event::Search(g, r) if g == self.search_generation => {
                self.search_loading = false;
                match r {
                    Ok(p) => {
                        self.search_results = true;
                        self.available = p;
                        self.selected = 0;
                        self.list_state = ListState::default();
                        self.status = format!("{} results", self.available.len());
                    }
                    Err(e) => {
                        self.search_results = false;
                        self.available = self.catalogue.clone();
                        self.status = format!("Search failed · showing cached matches: {e}");
                        self.log(&e);
                    }
                }
            }
            Event::Details(g, r) if g == self.generation => {
                self.detail_loading = false;
                self.detail_task = None;
                match r {
                    Ok(text) => {
                        if let Some(key) = &self.preview_key {
                            self.cache.retain(|(k, _)| k != key);
                            self.cache.push_front((key.clone(), text.clone()));
                            self.cache.truncate(64);
                        }
                        self.details = text;
                        self.persist();
                    }
                    Err(e) => {
                        self.details.push_str(&format!(
                            "\n\nCould not load details\n{e}\n\nPress Enter to retry."
                        ));
                    }
                }
            }
            Event::Output(CommandEvent::Output(text)) => self.log(&text),
            Event::Output(CommandEvent::Finished { success, code }) => {
                self.finish_operation(format!(
                    "{} · exit {:?}",
                    if success { "Completed" } else { "Failed" },
                    code
                ));
                self.log(&format!(
                    "{} · exit {}",
                    if success { "Completed" } else { "Failed" },
                    code.map(|c| c.to_string())
                        .unwrap_or_else(|| "signal".into())
                ));
                self.refresh(backend.clone(), tx.clone());
            }
            Event::Output(CommandEvent::Cancelled) => {
                self.finish_operation("Cancelled".into());
                self.log("Cancelled · refreshing actual Homebrew state");
                self.refresh(backend.clone(), tx.clone());
            }
            Event::Error(e) => {
                self.finish_operation(format!("Failed: {e}"));
                self.status = e.clone();
                self.log(&e);
            }
            Event::InputError(e) => {
                self.status = e;
                self.quit = true;
            }
            _ => {}
        }
        if inventory_result && self.loading == 0 && !self.load_failed {
            self.snapshot_updated = storage::now();
            self.persist();
            if self.status == "Refreshing Homebrew…" {
                self.status = "All caught up · select a package to explore".into();
            }
        }
        if self.loading == 0 && self.refresh_pending && !self.busy {
            self.refresh_pending = false;
            self.refresh(backend, tx);
        }
        self.sync_preview();
    }
    fn scroll(&mut self, down: bool, amount: usize) {
        match self.focus {
            Focus::Packages => {
                self.selected = if down {
                    (self.selected + amount).min(self.count().saturating_sub(1))
                } else {
                    self.selected.saturating_sub(amount)
                };
            }
            Focus::Details => {
                self.detail_scroll = if down {
                    self.detail_scroll.saturating_add(amount as u16)
                } else {
                    self.detail_scroll.saturating_sub(amount as u16)
                };
            }
            Focus::Output => {
                self.output_scroll = if down {
                    self.output_scroll.saturating_sub(amount)
                } else {
                    (self.output_scroll + amount).min(self.output.len().saturating_sub(1))
                };
            }
        }
    }
    pub fn mouse<B: BrewBackend>(
        &mut self,
        mouse: MouseEvent,
        backend: Arc<B>,
        tx: mpsc::Sender<Event>,
    ) {
        let hit = self
            .hits
            .iter()
            .rev()
            .find(|h| h.rect.contains(Position::new(mouse.column, mouse.row)))
            .copied();
        if let Some(hit) = hit {
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => match hit.action {
                    HitAction::Tab(tab) => self.change_tab(tab),
                    HitAction::Select(i) => {
                        self.focus = Focus::Packages;
                        self.editing = false;
                        self.selected = i;
                    }
                    HitAction::Focus(focus) => self.focus = focus,
                    HitAction::Key(KeyCode::Char('/')) if self.pending.is_none() && !self.help => {
                        self.editing = true;
                        self.focus = Focus::Packages;
                    }
                    HitAction::Key(code) => {
                        self.editing = false;
                        self.key(KeyEvent::new(code, KeyModifiers::NONE), backend, tx)
                    }
                },
                MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
                    if !self.help && self.pending.is_none() =>
                {
                    match hit.action {
                        HitAction::Select(_) => self.focus = Focus::Packages,
                        HitAction::Focus(f) => self.focus = f,
                        _ => return,
                    }
                    self.scroll(mouse.kind == MouseEventKind::ScrollDown, 3);
                }
                _ => {}
            }
        }
        self.sync_preview();
    }
    pub fn key<B: BrewBackend>(&mut self, key: KeyEvent, backend: Arc<B>, tx: mpsc::Sender<Event>) {
        self.handle_key(key, backend, tx);
        self.sync_preview();
    }
    fn handle_key<B: BrewBackend>(
        &mut self,
        key: KeyEvent,
        backend: Arc<B>,
        tx: mpsc::Sender<Event>,
    ) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            if self.busy {
                self.status = "Wait for the active operation before quitting".into();
            } else {
                self.quit = true;
            }
            return;
        }
        if self.stacks_visible && self.pending.is_none() {
            match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    self.stack_selected =
                        (self.stack_selected + 1).min(self.stacks.len().saturating_sub(1))
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.stack_selected = self.stack_selected.saturating_sub(1)
                }
                KeyCode::Char(action @ ('i' | 's' | 't')) if !self.busy => {
                    if let Some(stack) = self.stacks.get(self.stack_selected) {
                        match stack.plan(action, &self.installed) {
                            Ok(plan) => {
                                self.pending = Some(plan);
                                self.stacks_visible = false;
                            }
                            Err(error) => self.status = error.to_string(),
                        }
                    }
                }
                KeyCode::Esc | KeyCode::Char('S') => self.stacks_visible = false,
                _ => {}
            }
            return;
        }
        if self.history_visible {
            match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    self.history_scroll = self.history_scroll.saturating_add(1)
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.history_scroll = self.history_scroll.saturating_sub(1)
                }
                KeyCode::PageDown => self.history_scroll = self.history_scroll.saturating_add(10),
                KeyCode::PageUp => self.history_scroll = self.history_scroll.saturating_sub(10),
                KeyCode::Home => self.history_scroll = 0,
                _ => self.history_visible = false,
            }
            return;
        }
        if key.code == KeyCode::Char('X') && self.busy {
            if let Some(cancel) = self.cancellation.take() {
                let _ = cancel.send(());
                self.status = "Cancelling Homebrew operation…".into();
            }
            return;
        }
        if self.help {
            self.help = false;
            return;
        }
        if self.pending.is_some() {
            match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    self.pending_scroll = self.pending_scroll.saturating_add(1)
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.pending_scroll = self.pending_scroll.saturating_sub(1)
                }
                KeyCode::PageDown => self.pending_scroll = self.pending_scroll.saturating_add(10),
                KeyCode::PageUp => self.pending_scroll = self.pending_scroll.saturating_sub(10),
                KeyCode::Char('y') => {
                    let op = self.pending.take().unwrap();
                    self.busy = true;
                    let (cancel, mut cancelled) = tokio::sync::oneshot::channel();
                    self.cancellation = Some(cancel);
                    self.history.push_front(HistoryEntry {
                        operation: op.to_string(),
                        started: storage::now(),
                        finished: None,
                        result: "Running".into(),
                    });
                    self.history.truncate(100);
                    self.persist();
                    self.output_scroll = 0;
                    self.log(&format!("› {op}"));
                    self.status = "Running Homebrew operation…".into();
                    tokio::spawn(async move {
                        match backend.execute(op).await {
                            Ok(mut handle) => {
                                let mut cancellation_received = false;
                                loop {
                                    tokio::select! {
                                        request = &mut cancelled, if !cancellation_received => {
                                            cancellation_received = true;
                                            if request.is_ok() { handle.cancel(); }
                                        }
                                        event = handle.events.recv() => {
                                            let Some(event) = event else {
                                                let _ = tx.send(Event::Error("Command event stream closed".into())).await;
                                                break;
                                            };
                                            let terminal = !matches!(event, CommandEvent::Output(_));
                                            if tx.send(Event::Output(event)).await.is_err() || terminal { break; }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = tx.send(Event::Error(e.to_string())).await;
                            }
                        }
                    });
                }
                KeyCode::Esc | KeyCode::Char('n') => self.pending = None,
                _ => {}
            }
            return;
        }
        if self.editing {
            match key.code {
                KeyCode::Esc => {
                    self.editing = false;
                    self.filter.clear();
                    self.search_generation += 1;
                    self.search_loading = false;
                }
                KeyCode::Enter => {
                    self.editing = false;
                    if self.tab == Tab::Available && !self.filter.is_empty() {
                        self.search_generation += 1;
                        let g = self.search_generation;
                        let query = self.filter.clone();
                        self.search_loading = true;
                        self.status = format!("Searching {query}…");
                        tokio::spawn(async move {
                            let r = backend.search(&query).await.map_err(|e| e.to_string());
                            let _ = tx.send(Event::Search(g, r)).await;
                        });
                    }
                }
                KeyCode::Backspace => {
                    self.filter.pop();
                    self.search_generation += 1;
                    self.search_loading = false;
                }
                KeyCode::Char(c) => {
                    self.filter.push(c);
                    self.search_generation += 1;
                    self.search_loading = false;
                }
                _ => {}
            }
            self.selected = 0;
            self.list_state = ListState::default();
            return;
        }
        self.pending_scroll = 0;
        let id = self.id();
        match key.code {
            KeyCode::Char('q') => {
                if self.busy {
                    self.status = "Wait for the active operation before quitting".into();
                } else {
                    self.quit = true;
                }
            }
            KeyCode::Char('?') => self.help = true,
            KeyCode::Char('h') => self.history_visible = true,
            KeyCode::Char('S') => self.stacks_visible = true,
            KeyCode::Char('/') => {
                self.editing = true;
                self.focus = Focus::Packages;
            }
            KeyCode::Esc => {
                self.filter.clear();
                if self.tab == Tab::Available {
                    self.search_results = false;
                    self.available = self.catalogue.clone();
                }
                self.selected = 0;
                self.search_generation += 1;
                self.search_loading = false;
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = match (self.focus, key.code == KeyCode::Tab) {
                    (Focus::Packages, true) | (Focus::Output, false) => Focus::Details,
                    (Focus::Details, true) | (Focus::Packages, false) => Focus::Output,
                    _ => Focus::Packages,
                };
            }
            KeyCode::Char(c @ '1'..='5') => self.change_tab(Tab::ALL[c as usize - '1' as usize]),
            KeyCode::Left | KeyCode::Right => self.change_tab(
                Tab::ALL[(self.tab.index() + if key.code == KeyCode::Right { 1 } else { 4 }) % 5],
            ),
            KeyCode::Down | KeyCode::Char('j') => self.scroll(true, 1),
            KeyCode::Up | KeyCode::Char('k') => self.scroll(false, 1),
            KeyCode::Home => match self.focus {
                Focus::Packages => self.selected = 0,
                Focus::Details => self.detail_scroll = 0,
                Focus::Output => self.output_scroll = self.output.len().saturating_sub(1),
            },
            KeyCode::End => match self.focus {
                Focus::Packages => self.selected = self.count().saturating_sub(1),
                Focus::Details => {}
                Focus::Output => self.output_scroll = 0,
            },
            KeyCode::Char(']') => self.detail_scroll = self.detail_scroll.saturating_add(5),
            KeyCode::Char('[') => self.detail_scroll = self.detail_scroll.saturating_sub(5),
            KeyCode::PageUp => self.scroll(false, 10),
            KeyCode::PageDown => self.scroll(true, 10),
            KeyCode::Char('r') if !self.busy => self.refresh(backend.clone(), tx.clone()),
            KeyCode::Enter => {
                if self.preview_key.is_some() {
                    self.generation += 1;
                    self.detail_loading = true;
                    self.fetch_details(backend, tx);
                }
            }
            KeyCode::Char(c) if !self.busy => {
                let installed = id
                    .as_ref()
                    .is_some_and(|id| self.installed.iter().any(|p| &p.id == id));
                if c == 'v' {
                    if let Some(id) = id {
                        match crate::workflows::switch_version(&id, &self.installed, &self.services)
                        {
                            Ok(plan) => self.pending = Some(plan),
                            Err(error) => self.status = error.to_string(),
                        }
                    }
                    return;
                }
                self.pending = match c {
                    'U' => Some(Operation::Update),
                    'C' => Some(Operation::Cleanup),
                    'D' => Some(Operation::Doctor),
                    'i' if self.tab == Tab::Available => id.map(Operation::Install),
                    'x' if installed && self.tab != Tab::Services => id.map(Operation::Uninstall),
                    'u' if installed && self.tab != Tab::Services => id.map(Operation::Upgrade),
                    'p' if installed && self.tab != Tab::Services => {
                        id.filter(|id| id.kind == PackageKind::Formula).map(|id| {
                            if self.installed.iter().any(|p| p.id == id && p.pinned) {
                                Operation::Unpin(id)
                            } else {
                                Operation::Pin(id)
                            }
                        })
                    }
                    's' if self.tab == Tab::Services => id.map(Operation::Start),
                    't' if self.tab == Tab::Services => id.map(Operation::Stop),
                    'R' if self.tab == Tab::Services => id.map(Operation::Restart),
                    _ => None,
                };
            }
            _ => {}
        }
    }
}
impl Drop for App {
    fn drop(&mut self) {
        if let Some(task) = self.detail_task.take() {
            task.abort();
        }
    }
}
