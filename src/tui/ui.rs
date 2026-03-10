use ratatui::Frame;
use ratatui::widgets::Paragraph;

use super::app::App;

pub fn draw(frame: &mut Frame, _app: &App) {
    let area = frame.area();
    let placeholder = Paragraph::new("seed — press q to quit");
    frame.render_widget(placeholder, area);
}
