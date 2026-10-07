use crate::app::{App, Focus, Hit, HitAction, Tab};
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Wrap,
    },
};

fn styled(text: impl Into<String>, color: Color) -> Span<'static> {
    Span::styled(text.into(), Style::default().fg(color))
}
fn panel(
    palette: crate::theme::Palette,
    title: impl Into<String>,
    focused: bool,
) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(styled(
            format!(" {} ", title.into()),
            if focused {
                palette.accent
            } else {
                palette.muted
            },
        ))
        .border_style(Style::default().fg(if focused {
            palette.accent
        } else {
            palette.border
        }))
        .style(Style::default().bg(palette.panel).fg(palette.text))
}
pub fn popup(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}
fn hit(app: &mut App, rect: Rect, action: HitAction) {
    app.hits.push(Hit { rect, action });
}
fn button(frame: &mut Frame, app: &mut App, area: Rect, key: char, label: &str, accent: Color) {
    let palette = app.theme.palette();
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            styled(format!(" {key} "), accent),
            styled(format!("{label} "), palette.text),
        ]))
        .style(Style::default().bg(palette.selected)),
        area,
    );
    hit(app, area, HitAction::Key(KeyCode::Char(key)));
}
pub fn draw(frame: &mut Frame, app: &mut App) {
    let palette = app.theme.palette();
    app.hits.clear();
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(palette.bg).fg(palette.text)),
        area,
    );
    if area.width < 60 || area.height < 18 {
        frame.render_widget(
            Paragraph::new(
                "LazyBrew\n\nA little more room to brew.\nResize to at least 60 × 18.\nq to quit",
            )
            .style(Style::default().fg(palette.accent)),
            area,
        );
        return;
    }
    let item_count = app.count();
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(10),
        Constraint::Length(2),
        Constraint::Length(1),
    ])
    .split(area);
    let top = Layout::horizontal([Constraint::Min(25), Constraint::Length(24)]).split(rows[0]);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "  lazybrew",
                Style::default()
                    .fg(palette.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            styled("  /  a better way to brew", palette.muted),
        ])),
        top[0],
    );
    let state = if app.busy {
        "● RUNNING"
    } else if app.loading > 0 || app.search_loading {
        "◌ SYNCING"
    } else {
        "● READY"
    };
    frame.render_widget(
        Paragraph::new(format!("{state}    v{}", env!("CARGO_PKG_VERSION"))).style(
            Style::default().fg(if app.busy {
                palette.warning
            } else {
                palette.accent
            }),
        ),
        top[1],
    );
    let columns =
        Layout::horizontal([Constraint::Percentage(36), Constraint::Percentage(64)]).split(rows[1]);
    let left = Layout::vertical([Constraint::Length(8), Constraint::Min(3)]).split(columns[0]);
    let right = Layout::vertical([Constraint::Percentage(66), Constraint::Percentage(34)])
        .split(columns[1]);

    let nav = panel(palette, "WORKSPACE", false);
    let nav_inner = nav.inner(left[0]);
    frame.render_widget(nav, left[0]);
    for (index, tab) in Tab::ALL.iter().enumerate() {
        let rect = Rect::new(nav_inner.x, nav_inner.y + index as u16, nav_inner.width, 1);
        let count = app.tab_count(*tab);
        let label = format!(
            " {}  {:<12} {:>4}",
            index + 1,
            tab.label(),
            if *tab == Tab::Available && count == 0 {
                "⌕".into()
            } else {
                count.to_string()
            }
        );
        frame.render_widget(
            Paragraph::new(label).style(if app.tab == *tab {
                Style::default()
                    .fg(palette.accent)
                    .bg(palette.selected)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(palette.muted).bg(palette.panel)
            }),
            rect,
        );
        hit(app, rect, HitAction::Tab(*tab));
    }
    let hint = Rect::new(
        nav_inner.x + 1,
        nav_inner.y + 5,
        nav_inner.width.saturating_sub(2),
        1,
    );
    frame.render_widget(
        Paragraph::new("← → switch · click to open").style(Style::default().fg(palette.muted)),
        hint,
    );

    let title = format!(
        "{}  {}/{}",
        app.tab.label(),
        if item_count > 0 { app.selected + 1 } else { 0 },
        item_count
    );
    let list_block = panel(palette, title, app.focus == Focus::Packages);
    let list_inner = list_block.inner(left[1]);
    frame.render_widget(list_block, left[1]);
    hit(app, left[1], HitAction::Focus(Focus::Packages));
    let filter_rect = Rect::new(list_inner.x, list_inner.y, list_inner.width, 1);
    let filter_text = if app.editing {
        format!(" / {}▏", app.filter)
    } else if app.filter.is_empty() {
        " / Search…".into()
    } else {
        format!(" / {}", app.filter)
    };
    frame.render_widget(
        Paragraph::new(filter_text).style(
            Style::default()
                .fg(if app.editing {
                    palette.accent
                } else {
                    palette.muted
                })
                .bg(if app.editing {
                    palette.selected
                } else {
                    palette.panel
                }),
        ),
        filter_rect,
    );
    hit(app, filter_rect, HitAction::Key(KeyCode::Char('/')));
    let list_area = Rect::new(
        list_inner.x,
        list_inner.y + 1,
        list_inner.width,
        list_inner.height.saturating_sub(1),
    );
    let items: Vec<ListItem<'static>> = if app.tab == Tab::Services {
        app.service_rows()
            .iter()
            .map(|s| {
                let color = match s.status.as_str() {
                    "started" => palette.accent,
                    "error" => palette.error,
                    _ => palette.muted,
                };
                ListItem::new(Line::from(vec![
                    styled(" ● ", color),
                    styled(s.name.clone(), palette.text),
                    styled(format!("  {}", s.status), color),
                ]))
            })
            .collect()
    } else {
        app.packages()
            .iter()
            .map(|p| {
                ListItem::new(Line::from(vec![
                    styled(
                        if p.outdated {
                            " ↑ "
                        } else if p.pinned {
                            " ◆ "
                        } else {
                            " · "
                        },
                        if p.outdated {
                            palette.warning
                        } else {
                            palette.muted
                        },
                    ),
                    styled(p.id.name().to_owned(), palette.text),
                    styled(
                        format!("  {}", p.installed.first().unwrap_or(&p.version)),
                        palette.muted,
                    ),
                ]))
            })
            .collect()
    };
    app.list_state.select(if items.is_empty() {
        None
    } else {
        Some(app.selected)
    });
    if items.is_empty() {
        let message = if app.loading > 0 {
            "\n  Fetching your workspace…"
        } else if app.search_loading {
            "\n  Searching Homebrew…"
        } else if app.tab == Tab::Available {
            "\n  Find your next tool.\n\n  Type / and a name,\n  then press Enter."
        } else if app.tab == Tab::Outdated && app.filter.is_empty() {
            "\n  Everything is up to date.\n\n  A fresh cup, a clean slate."
        } else {
            "\n  No matching items.\n\n  Clear the filter or refresh."
        };
        frame.render_widget(
            Paragraph::new(message)
                .style(Style::default().fg(palette.muted))
                .wrap(Wrap { trim: false }),
            list_area,
        );
    } else {
        frame.render_stateful_widget(
            List::new(items).highlight_symbol("▎").highlight_style(
                Style::default()
                    .bg(palette.selected)
                    .add_modifier(Modifier::BOLD),
            ),
            list_area,
            &mut app.list_state,
        );
        for row in 0..list_area.height {
            let index = app.list_state.offset() + row as usize;
            if index < item_count {
                hit(
                    app,
                    Rect::new(list_area.x, list_area.y + row, list_area.width, 1),
                    HitAction::Select(index),
                );
            }
        }
        if item_count > list_area.height as usize {
            let mut scrollbar = ScrollbarState::new(item_count)
                .position(app.selected)
                .viewport_content_length(list_area.height as usize);
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(None)
                    .end_symbol(None)
                    .thumb_style(Style::default().fg(palette.accent))
                    .track_style(Style::default().fg(palette.border)),
                left[1],
                &mut scrollbar,
            );
        }
    }

    let detail_block = panel(
        palette,
        if app.detail_loading {
            "INSPECT  ·  fetching…"
        } else {
            "INSPECT"
        },
        app.focus == Focus::Details,
    );
    let detail_inner = detail_block.inner(right[0]);
    frame.render_widget(detail_block, right[0]);
    hit(app, right[0], HitAction::Focus(Focus::Details));
    let detail_rows =
        Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).split(detail_inner);
    let name = app
        .id()
        .map(|p| p.name().to_owned())
        .unwrap_or_else(|| "Your Homebrew, in focus.".into());
    frame.render_widget(
        Paragraph::new(format!(" {name}")).style(
            Style::default()
                .fg(palette.warning)
                .add_modifier(Modifier::BOLD),
        ),
        detail_rows[0],
    );
    let text = if app.details.is_empty() {
        "Select a package or service on the left.\n\nDetails follow your selection automatically.\nClick a panel to focus it; scroll to explore.\n\n1–5  workspaces      Tab  next panel\n /   search          ?    all shortcuts"
    } else {
        &app.details
    };
    let mut lines = Vec::new();
    let width = detail_rows[1].width.saturating_sub(2).max(1) as usize;
    for line in text.lines() {
        let heading = !line.is_empty()
            && line.len() < 32
            && line
                .chars()
                .all(|c| c.is_ascii_uppercase() || c == ' ' || c == '/');
        let color = if heading {
            palette.accent
        } else {
            palette.text
        };
        for wrapped in wrap_line(line, width) {
            lines.push(Line::from(styled(format!(" {wrapped}"), color)));
        }
    }
    let max_scroll = lines
        .len()
        .saturating_sub(detail_rows[1].height as usize)
        .min(u16::MAX as usize) as u16;
    app.detail_scroll = app.detail_scroll.min(max_scroll);
    frame.render_widget(
        Paragraph::new(lines).scroll((app.detail_scroll, 0)),
        detail_rows[1],
    );

    let output_block = panel(
        palette,
        if app.busy {
            "ACTIVITY  ·  running"
        } else {
            "ACTIVITY"
        },
        app.focus == Focus::Output,
    );
    let output_inner = output_block.inner(right[1]);
    frame.render_widget(output_block, right[1]);
    hit(app, right[1], HitAction::Focus(Focus::Output));
    if app.output.is_empty() {
        frame.render_widget(
            Paragraph::new("\n No operations yet.\n Command output will stream here.")
                .style(Style::default().fg(palette.muted)),
            output_inner,
        );
    } else {
        let visible = output_inner.height as usize;
        app.output_scroll = app
            .output_scroll
            .min(app.output.len().saturating_sub(visible));
        let end = app.output.len().saturating_sub(app.output_scroll);
        let lines: Vec<Line> = app
            .output
            .iter()
            .skip(end.saturating_sub(visible))
            .take(visible)
            .map(|s| {
                Line::from(styled(
                    format!(" {s}"),
                    if s.starts_with('›') {
                        palette.warning
                    } else if s.starts_with("Completed") {
                        palette.accent
                    } else if s.starts_with("Failed") || s.contains("Error:") {
                        palette.error
                    } else {
                        palette.muted
                    },
                ))
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), output_inner);
    }

    let actions: Vec<(char, &str)> = if app.busy {
        vec![('X', "Cancel"), ('h', "History")]
    } else {
        match app.tab {
            Tab::Services => vec![
                ('s', "Start"),
                ('t', "Stop"),
                ('R', "Restart"),
                ('r', "Refresh"),
            ],
            Tab::Available => vec![('i', "Install"), ('/', "Search"), ('r', "Refresh")],
            _ => vec![
                ('u', "Upgrade"),
                ('x', "Remove"),
                ('p', "Pin"),
                ('v', "Activate"),
                ('r', "Refresh"),
            ],
        }
    };
    let mut x = rows[2].x + 1;
    for (key, label) in actions {
        let width = label.len() as u16 + 4;
        if x + width <= rows[2].right() {
            button(
                frame,
                app,
                Rect::new(x, rows[2].y, width, 1),
                key,
                label,
                palette.accent,
            );
            x += width + 1;
        }
    }
    x = rows[2].x + 1;
    for (key, label) in [
        ('U', "Update"),
        ('C', "Cleanup"),
        ('D', "Doctor"),
        ('S', "Stacks"),
        ('h', "History"),
        ('?', "Help"),
        ('q', "Quit"),
    ] {
        let width = label.len() as u16 + 4;
        if x + width <= rows[2].right() {
            button(
                frame,
                app,
                Rect::new(x, rows[2].y + 1, width, 1),
                key,
                label,
                palette.warning,
            );
            x += width + 1;
        }
    }
    frame.render_widget(
        Paragraph::new(format!(" {}", app.status)).style(Style::default().fg(palette.muted)),
        rows[3],
    );

    if app.help {
        app.hits.clear();
        let rect = popup(area, 88, 27);
        frame.render_widget(Clear, rect);
        frame.render_widget(Paragraph::new("\n  MOVE AROUND\n  1–5 / ← →        Switch workspace\n  Tab / Shift-Tab  Focus another panel\n  j k / ↑ ↓        Navigate or scroll focused panel\n  PgUp / PgDn      Scroll a page · Home / End list boundaries\n  Mouse            Click workspaces, rows, panels and actions\n  Wheel            Scroll the panel under the pointer\n\n  EXPLORE & ACT\n  /                Filter · Discover: Enter to search\n  Enter            Reload selected details\n  i / x / u / p    Install / remove / upgrade / pin formula\n  s / t / R        Start / stop / restart service\n  U / C / D        Homebrew update / cleanup / doctor\n  r                Refresh workspace\n  v / S            Activate runtime version / development stacks\n  X / h            Cancel running command / operation history\n  Esc              Clear filter / cancel confirmation\n  q / Ctrl-C       Quit after the active command finishes\n\n  Every operation asks for confirmation.  [ Close / Esc ]").style(Style::default().bg(palette.panel).fg(palette.text)).block(panel(palette, "MAKE YOURSELF AT HOME", true)).wrap(Wrap { trim: false }), rect);
        hit(app, rect, HitAction::Key(KeyCode::Esc));
    }
    if app.stacks_visible {
        app.hits.clear();
        let rect = popup(area, 88, 27);
        frame.render_widget(Clear, rect);
        let content = Rect::new(
            rect.x + 1,
            rect.y + 1,
            rect.width.saturating_sub(2),
            rect.height.saturating_sub(4),
        );
        let mut lines = Vec::new();
        let mut selected_line = 0;
        for line in ["j/k Select · i Setup · s Start · t Stop · Esc Close", ""] {
            lines.extend(
                wrap_line(line, content.width as usize)
                    .into_iter()
                    .map(Line::from),
            );
        }
        if app.stacks.is_empty() {
            lines.extend(
                wrap_line(
                    "Define [[stacks]] in config.toml. See config.example.toml.",
                    content.width as usize,
                )
                .into_iter()
                .map(Line::from),
            );
        }
        for (index, stack) in app.stacks.iter().enumerate() {
            if index == app.stack_selected {
                selected_line = lines.len();
            }
            let text = format!(
                "{} {}\n  Packages: {}\n  Services: {}",
                if index == app.stack_selected {
                    "›"
                } else {
                    " "
                },
                stack.name,
                stack.formulae.join(", "),
                stack.services.join(", ")
            );
            lines.extend(
                text.lines()
                    .flat_map(|line| wrap_line(line, content.width as usize))
                    .map(Line::from),
            );
        }
        let scroll = selected_line
            .saturating_sub(content.height as usize / 2)
            .min(lines.len().saturating_sub(content.height as usize)) as u16;
        frame.render_widget(panel(palette, "DEVELOPMENT STACKS", true), rect);
        frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), content);
        for (offset, key, label) in [(0, 'i', "Setup"), (18, 's', "Start"), (36, 't', "Stop")] {
            button(
                frame,
                app,
                Rect::new(rect.x + 2 + offset, rect.bottom() - 2, 16, 1),
                key,
                label,
                palette.accent,
            );
        }
    }
    if app.history_visible {
        app.hits.clear();
        let rect = popup(area, 90, 24);
        frame.render_widget(Clear, rect);
        let text = if app.history.is_empty() {
            "No operations recorded yet.".to_string()
        } else {
            app.history
                .iter()
                .map(|entry| format!("{} · {}\n{}", entry.started, entry.result, entry.operation))
                .collect::<Vec<_>>()
                .join("\n\n")
        };
        let lines: Vec<Line> = text
            .lines()
            .flat_map(|line| wrap_line(line, rect.width.saturating_sub(2) as usize))
            .map(Line::from)
            .collect();
        app.history_scroll = app.history_scroll.min(
            lines
                .len()
                .saturating_sub(rect.height.saturating_sub(2) as usize) as u16,
        );
        frame.render_widget(
            Paragraph::new(lines)
                .scroll((app.history_scroll, 0))
                .block(panel(
                    palette,
                    "OPERATION HISTORY · j/k to scroll · Esc to close",
                    true,
                )),
            rect,
        );
        hit(app, rect, HitAction::Key(KeyCode::Esc));
    }
    if let Some(op) = &app.pending {
        let description = op.to_string();
        app.hits.clear();
        let height = description
            .lines()
            .count()
            .saturating_add(8)
            .min(area.height.saturating_sub(2) as usize) as u16;
        let rect = popup(area, 88, height.max(10));
        frame.render_widget(Clear, rect);
        frame.render_widget(
            panel(
                palette,
                "ONE QUICK CONFIRMATION · j/k or PgUp/PgDn to review",
                true,
            ),
            rect,
        );
        let content = Rect::new(
            rect.x + 1,
            rect.y + 1,
            rect.width.saturating_sub(2),
            rect.height.saturating_sub(5),
        );
        let text = format!(
            "{description}\n\nThis will run a Homebrew operation. Review every step before confirming. Completed steps are not rolled back on failure or cancellation."
        );
        let lines: Vec<Line> = text
            .lines()
            .flat_map(|line| wrap_line(line, content.width as usize))
            .map(Line::from)
            .collect();
        app.pending_scroll = app
            .pending_scroll
            .min(lines.len().saturating_sub(content.height as usize) as u16);
        frame.render_widget(
            Paragraph::new(lines).scroll((app.pending_scroll, 0)),
            content,
        );
        button(
            frame,
            app,
            Rect::new(rect.x + 3, rect.bottom() - 3, 15, 1),
            'y',
            "Confirm",
            palette.accent,
        );
        button(
            frame,
            app,
            Rect::new(rect.x + 20, rect.bottom() - 3, 14, 1),
            'n',
            "Cancel",
            palette.warning,
        );
    }
}

/// Wrap at word boundaries, splitting long URLs without losing any characters.
fn wrap_line(text: &str, width: usize) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };
        if Line::from(candidate.as_str()).width() <= width {
            current = candidate;
            continue;
        }
        if !current.is_empty() {
            result.push(std::mem::take(&mut current));
        }
        for ch in word.chars() {
            if Line::from(current.as_str()).width() + Line::from(ch.to_string()).width() > width
                && !current.is_empty()
            {
                result.push(std::mem::take(&mut current));
            }
            current.push(ch);
        }
    }
    if !current.is_empty() || result.is_empty() {
        result.push(current);
    }
    result
}
