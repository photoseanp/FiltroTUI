use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use super::list_offset;
use crate::app::{App, LogKind, Target};

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);

    draw_connection(f, cols[0], app);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(cols[1]);
    draw_log(f, right[0], app);
    draw_raw(f, right[1], app);
}

fn draw_connection(f: &mut Frame, area: Rect, app: &App) {
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(3)])
        .split(area);

    let state = match &app.care.connected {
        Some(p) => Span::styled(format!("Connected: {}", p), Style::default().fg(Color::Green)),
        None => Span::styled("Disconnected", Style::default().fg(Color::Red)),
    };
    let info = Paragraph::new(vec![
        Line::from(state),
        Line::from(format!("Baud: {} (Left/Right), 8N1", app.baud())),
        Line::from("Enter connect | d disconnect | F5 refresh"),
    ])
    .block(Block::default().borders(Borders::ALL).title(" Connection "));
    f.render_widget(info, parts[0]);

    let items: Vec<ListItem> = if app.care.ports.is_empty() {
        vec![ListItem::new("(no serial ports found)")]
    } else {
        app.care.ports.iter().map(|p| ListItem::new(p.clone())).collect()
    };
    let list_area = parts[1];
    let n = app.care.ports.len();
    let visible = list_area.height.saturating_sub(2) as usize;
    let sel = app.care.selected.min(n.saturating_sub(1));
    let off = list_offset(sel, visible);
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Ports "))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");
    let mut st = ListState::default().with_offset(off);
    if n > 0 {
        st.select(Some(sel));
    }
    f.render_stateful_widget(list, list_area, &mut st);

    for r in 0..visible.min(n.saturating_sub(off)) {
        app.add_hit(
            Rect {
                x: list_area.x + 1,
                y: list_area.y + 1 + r as u16,
                width: list_area.width.saturating_sub(2),
                height: 1,
            },
            Target::Port(off + r),
        );
    }
}

fn draw_log(f: &mut Frame, area: Rect, app: &App) {
    let visible = area.height.saturating_sub(2) as usize;
    let width = area.width.saturating_sub(2).max(1) as usize;
    // Walk back from the newest entry until the (wrapped) lines fill the panel.
    let mut used = 0usize;
    let mut start = app.log.len();
    while start > 0 {
        let l = &app.log[start - 1];
        let h = (16 + l.text.chars().count()) / width + 1;
        if used + h > visible {
            break;
        }
        used += h;
        start -= 1;
    }
    let lines: Vec<Line> = app.log[start..]
        .iter()
        .map(|l| {
            let (tag, color) = match l.kind {
                LogKind::Tx => ("TX ", Color::Cyan),
                LogKind::Rx => ("RX ", Color::Green),
                LogKind::Info => ("-- ", Color::Gray),
                LogKind::Err => ("ERR", Color::Red),
            };
            Line::from(vec![
                Span::styled(format!("{:8.1}s ", l.t), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{} ", tag), Style::default().fg(color)),
                Span::raw(l.text.clone()),
            ])
        })
        .collect();
    let p = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" RS485 poll log (x clear) "),
        );
    f.render_widget(p, area);
}

fn draw_raw(f: &mut Frame, area: Rect, app: &App) {
    let (text, title) = match &app.care.raw {
        Some(b) => (format!("{}_", b), " Raw command - Enter send / Esc cancel "),
        None => (String::from("press / to type a raw command"), " Raw command "),
    };
    let style = if app.care.raw.is_some() {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let p = Paragraph::new(text)
        .style(style)
        .block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(p, area);
}
