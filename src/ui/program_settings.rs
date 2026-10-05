use ratatui::{
    layout::Rect,
    widgets::{Block, Borders},
    Frame,
};

use crate::app::App;

pub fn draw(f: &mut Frame, area: Rect, _app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Program Settings ");
    f.render_widget(block, area);
}
