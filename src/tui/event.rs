use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};

use super::app::App;

pub enum Action {
    Continue,
    Quit,
}

pub fn handle_events(_app: &mut App) -> std::io::Result<Action> {
    if !event::poll(std::time::Duration::from_millis(16))? {
        return Ok(Action::Continue);
    }
    let Event::Key(key) = event::read()? else {
        return Ok(Action::Continue);
    };
    if key.kind != KeyEventKind::Press {
        return Ok(Action::Continue);
    }
    match key.code {
        KeyCode::Char('q') => Ok(Action::Quit),
        _ => Ok(Action::Continue),
    }
}
