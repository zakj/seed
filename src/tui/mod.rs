mod app;
mod event;
mod markdown;
mod ui;

use std::io;
use std::panic;

use ratatui::crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use crate::error::Error;
use crate::store::Store;

pub fn run(store: Store) -> Result<(), Error> {
    enable_raw_mode()?;
    execute!(io::stderr(), EnterAlternateScreen, EnableMouseCapture)?;

    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        original_hook(info);
    }));

    let backend = ratatui::backend::CrosstermBackend::new(io::stderr());
    let mut terminal = ratatui::Terminal::new(backend)?;

    let app = app::App::new(store)?;
    let result = run_loop(&mut terminal, app);

    restore_terminal()?;
    result
}

fn run_loop(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<io::Stderr>>,
    mut app: app::App,
) -> Result<(), Error> {
    loop {
        terminal.draw(|frame| ui::draw(frame, &mut app))?;
        if let event::Action::Quit = event::handle_events(&mut app)? {
            return Ok(());
        }
        app.maybe_refresh();
    }
}

fn restore_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(io::stderr(), LeaveAlternateScreen, DisableMouseCapture)?;
    Ok(())
}
