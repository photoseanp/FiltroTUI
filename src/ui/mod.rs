mod care_center;
mod detail;
mod overview;
mod program_settings;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph, Tabs},
    Frame,
};

use crate::app::{App, Tab, Target};

/// First visible row of a list when `sel` has to stay visible and the list starts at the top.
pub fn list_offset(sel: usize, visible: usize) -> usize {
    if visible == 0 || sel < visible {
        0
    } else {
        sel + 1 - visible
    }
}

/// Registers one click region per tab title of a `Tabs` widget (default padding 1, divider 1).
pub fn tab_hits<F: Fn(usize) -> Target>(app: &App, block_area: Rect, titles: &[&str], make: F) {
    let mut x = block_area.x + 1;
    let y = block_area.y + 1;
    for (i, t) in titles.iter().enumerate() {
        let w = t.chars().count() as u16 + 2;
        app.add_hit(Rect { x, y, width: w, height: 1 }, make(i));
        x += w + 1;
    }
}

pub fn draw(f: &mut Frame, app: &App) {
    app.hits.borrow_mut().clear();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(f.area());

    let titles: Vec<Line> = Tab::ALL.iter().map(|t| Line::from(t.title())).collect();
    let conn = match &app.care.connected {
        Some(p) => format!(" FiltroTUI | connected: {} @ {} ", p, app.baud()),
        None => " FiltroTUI | disconnected ".to_string(),
    };
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title(conn))
        .select(app.tab.index())
        .style(Style::default().fg(Color::Gray))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        );
    f.render_widget(tabs, chunks[0]);
    let names: Vec<&str> = Tab::ALL.iter().map(|t| t.title()).collect();
    tab_hits(app, chunks[0], &names, Target::Tab);

    match app.tab {
        Tab::Overview => {
            overview::draw(f, chunks[1], app);
            if app.detail_open {
                detail::draw(f, chunks[1], app);
            }
        }
        Tab::ProgramSettings => program_settings::draw(f, chunks[1], app),
        Tab::CareCenter => care_center::draw(f, chunks[1], app),
    }

    let tab_hint = match app.tab {
        Tab::Overview => "m mode | y scale | l axis | Enter details | mouse ok",
        Tab::ProgramSettings => "mouse: click / right click / wheel",
        Tab::CareCenter => "mouse: click port, again = connect",
    };
    let hint = Paragraph::new(format!("Tab: switch | q: quit | {} | {}", tab_hint, app.status))
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(hint, chunks[2]);
}
