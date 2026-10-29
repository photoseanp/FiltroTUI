use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use super::list_offset;
use crate::app::App;
use crate::export;

fn centered(area: Rect, pw: u16, h: u16) -> Rect {
    let w = (area.width * pw / 100).max(20).min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

pub fn draw_save(f: &mut Frame, area: Rect, app: &App) {
    let Some(d) = &app.save_dlg else { return };
    let rect = centered(area, 80, 16);
    f.render_widget(Clear, rect);

    let field = |idx: usize, label: &str, val: &str| -> Line<'static> {
        let active = d.field == idx && !d.ask_png;
        let style = if active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        Line::from(vec![
            Span::styled(
                format!("{} {:<12}", if active { ">" } else { " " }, label),
                style,
            ),
            Span::styled(
                format!("{}{}", val, if active { "_" } else { "" }),
                style,
            ),
        ])
    };

    let stem = export::stem_of(&d.name);
    let mut lines: Vec<Line> = vec![
        Line::from(format!(
            "Saving {} selected measurement(s): raw data and the current analysis settings",
            d.count
        )),
        Line::from(""),
        field(0, "Folder:", &d.folder),
        field(1, "File name:", &d.name),
        field(2, "Notes:", &d.notes),
        Line::from(""),
        Line::from(Span::styled(
            format!("Creates: {}.txt (raw data); the {}.png chart is optional", stem, stem),
            Style::default().fg(Color::Gray),
        )),
    ];
    if let Some(e) = &d.error {
        lines.push(Line::from(Span::styled(
            format!("Error: {}", e),
            Style::default().fg(Color::Red),
        )));
    }
    lines.push(Line::from(""));
    if d.ask_png {
        lines.push(Line::from(Span::styled(
            "Also save the PNG chart (efficiency + distribution)?  y = yes | n / Enter = txt only | Esc = back",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "Tab / Up / Down: next field | Enter: continue | Esc: cancel",
            Style::default().fg(Color::DarkGray),
        )));
    }

    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Save measurements "),
            ),
        rect,
    );
}

pub fn draw_open(f: &mut Frame, area: Rect, app: &App) {
    let Some(d) = &app.open_dlg else { return };
    let rect = centered(area, 85, area.height.saturating_sub(4).max(10));
    f.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Open saved measurements ");
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let mut head: Vec<Line> = Vec::new();
    if d.editing {
        head.push(Line::from(Span::styled(
            format!("Folder: {}_", d.buf),
            Style::default().fg(Color::Yellow),
        )));
        head.push(Line::from("Type the folder path, Enter: confirm, Esc: cancel"));
    } else {
        head.push(Line::from(format!("Folder: {}", d.dir)));
        head.push(Line::from(format!(
            "{} file(s), {} marked (Enter opens the marked files, or the current one if none is marked)",
            d.items.len(),
            d.marks.len()
        )));
    }
    if let Some(e) = &d.error {
        head.push(Line::from(Span::styled(
            e.clone(),
            Style::default().fg(Color::Red),
        )));
    }
    f.render_widget(Paragraph::new(head).wrap(Wrap { trim: false }), parts[0]);

    let items: Vec<ListItem> = d
        .items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            ListItem::new(format!(
                "[{}] {}",
                if d.marks.contains(&i) { "x" } else { " " },
                it.label
            ))
        })
        .collect();
    let n = items.len();
    let visible = parts[1].height as usize;
    let sel = d.sel.min(n.saturating_sub(1));
    let off = list_offset(sel, visible);
    let list = List::new(items)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");
    let mut st = ListState::default().with_offset(off);
    if n > 0 && !d.editing {
        st.select(Some(sel));
    }
    f.render_stateful_widget(list, parts[1], &mut st);

    f.render_widget(
        Paragraph::new("Up/Down: move | Space: mark | a: all | n: none | Enter: open | f: folder | Esc: close")
            .style(Style::default().fg(Color::DarkGray)),
        parts[2],
    );
}
