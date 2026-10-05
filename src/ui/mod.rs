mod care_center;
mod overview;
mod program_settings;

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph, Tabs},
    Frame,
};

use crate::app::{App, Tab};

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(f.area());

    let titles: Vec<Line> = Tab::ALL.iter().map(|t| Line::from(t.title())).collect();
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title(" FiltroTUI "))
        .select(app.tab.index())
        .style(Style::default().fg(Color::Gray))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        );
    f.render_widget(tabs, chunks[0]);

    match app.tab {
        Tab::Overview => overview::draw(f, chunks[1], app),
        Tab::ProgramSettings => program_settings::draw(f, chunks[1], app),
        Tab::CareCenter => care_center::draw(f, chunks[1], app),
    }

    let hint = Paragraph::new("Tab/Shift+Tab: switch window | q: quit")
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(hint, chunks[2]);
}
