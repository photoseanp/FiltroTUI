use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders},
    Frame,
};

use crate::app::App;

pub fn draw(f: &mut Frame, area: Rect, _app: &App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(area);

    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(34),
            Constraint::Percentage(33),
        ])
        .split(rows[0]);

    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[1]);

    let panel = |title: &'static str| Block::default().borders(Borders::ALL).title(title);

    f.render_widget(panel(" Flow rate "), top[0]);
    f.render_widget(panel(" Differential pressure "), top[1]);
    f.render_widget(panel(" Circulation pump "), top[2]);
    f.render_widget(panel(" Fractional efficiency "), bottom[0]);
    f.render_widget(panel(" Particle distribution "), bottom[1]);
}
