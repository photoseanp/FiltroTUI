use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs, Wrap},
    Frame,
};

use crate::app::App;
use crate::settings::{AutoManual, Field, Page, RunMode};

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let titles: Vec<Line> = Page::ALL.iter().map(|p| Line::from(p.title())).collect();
    let pages = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Program Settings  ( [ ] switch page ) "),
        )
        .select(app.ps.page.index())
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    f.render_widget(pages, rows[0]);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(rows[1]);

    draw_form(f, cols[0], app);
    draw_summary(f, cols[1], app);

    let hint = "Up/Down select | Left/Right change | Enter edit/cycle | Space toggle | r defaults";
    f.render_widget(
        Paragraph::new(hint).style(Style::default().fg(Color::DarkGray)),
        rows[2],
    );
}

fn draw_form(f: &mut Frame, area: Rect, app: &App) {
    let s = &app.settings;
    let fields = s.fields(app.ps.page);
    let sel = app.ps.selected.min(fields.len().saturating_sub(1));

    let items: Vec<ListItem> = fields
        .iter()
        .enumerate()
        .map(|(i, fld)| {
            let value = match (&app.ps.editing, i == sel) {
                (Some(buf), true) => format!("{}_", buf),
                _ => s.display(*fld),
            };
            let dim = match fld {
                Field::Channel(c) => !s.channels[*c].enabled,
                _ => false,
            };
            let style = if dim {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::raw(format!("{:<22}", s.label(*fld))),
                Span::styled(value, style),
            ]))
        })
        .collect();

    let title = if app.ps.editing.is_some() {
        format!(" {} - typing (Enter ok / Esc cancel) ", app.ps.page.title())
    } else {
        format!(" {} ", app.ps.page.title())
    };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");
    let mut state = ListState::default().with_selected(Some(sel));
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_summary(f: &mut Frame, area: Rect, app: &App) {
    let s = &app.settings;
    let r = &s.run;
    let mut lines: Vec<Line> = Vec::new();

    match app.ps.page {
        Page::Run => {
            lines.push(Line::from(format!("Mode:             {}", r.mode.label())));
            lines.push(Line::from(format!("Sampler flow:     {} ml/min", s.flow_ml_min())));
            lines.push(Line::from(format!("Volume per count: {:.2} ml", s.sample_volume_ml())));
            let cycle = (r.counting_time_s + r.interval_s).max(1) as f64;
            match r.mode {
                RunMode::Initial => lines.push(Line::from(format!(
                    "Counts in 60 min: ~{}",
                    (3600.0 / cycle).floor()
                ))),
                RunMode::Formal => lines.push(Line::from(format!(
                    "Counts in test:   ~{}",
                    (r.test_time_min as f64 * 60.0 / cycle).floor()
                ))),
                RunMode::Single => {
                    let n = if r.sample_mode == AutoManual::Auto { r.detections } else { 1 };
                    lines.push(Line::from(format!("Counts per start: {}", n)));
                }
            }
            lines.push(Line::from(""));
            lines.push(Line::from("Instrument stores max 500 counts (oldest overwritten)."));
        }
        Page::Channels => {
            let sizes = s.enabled_sizes();
            lines.push(Line::from(format!("Enabled: {} / 16", sizes.len())));
            let txt: Vec<String> = sizes.iter().map(|v| format!("{}", v)).collect();
            lines.push(Line::from(format!("Sorted sizes (um): {}", txt.join(", "))));
            lines.push(Line::from(""));
            lines.push(Line::from("The instrument sorts sizes automatically."));
            lines.push(Line::from("Space: enable/disable | Enter: type size"));
            lines.push(Line::from("Default sizes are placeholders."));
        }
        Page::Flush => {
            lines.push(Line::from(format!("Cleaning time: {} s", s.flush.time_s)));
            lines.push(Line::from(format!("Direction: {}", s.flush.direction.label())));
            lines.push(Line::from(format!("Sensor: {}", s.flush.sensor.label())));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Never flush while the samplers are connected to the test system! A wrong direction can damage the metering pump.",
                Style::default().fg(Color::Yellow),
            )));
            lines.push(Line::from(
                "Forward: inlet in cleaning fluid. Reverse: outlet in cleaning fluid (unclogging).",
            ));
        }
    }

    lines.push(Line::from(""));
    let warns = s.warnings();
    if warns.is_empty() {
        lines.push(Line::from(Span::styled("Settings OK", Style::default().fg(Color::Green))));
    } else {
        for w in warns {
            lines.push(Line::from(Span::styled(
                format!("! {}", w),
                Style::default().fg(Color::Red),
            )));
        }
    }

    let p = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" Summary "))
        .wrap(Wrap { trim: true });
    f.render_widget(p, area);
}
