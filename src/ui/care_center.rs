use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders},
    Frame,
};

use crate::app::App;

pub fn draw(f: &mut Frame, area: Rect, _app: &App) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);

    f.render_widget(
        Block::default().borders(Borders::ALL).title(" Sensor status "),
        cols[0],
    );
    f.render_widget(
        Block::default().borders(Borders::ALL).title(" RS485 poll log "),
        cols[1],
    );
}
