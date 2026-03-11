mod app;
mod event;
mod keys;
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
use crate::ops::{self, Edits};
use crate::store::Store;
use crate::task::TaskId;

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
        match event::handle_events(&mut app)? {
            event::Action::Quit => return Ok(()),
            event::Action::EditDescription(id) => {
                if let Err(e) = edit_description(terminal, &mut app, id) {
                    app.set_status(e.to_string());
                }
            }
            event::Action::Continue => {}
        }
        app.maybe_refresh();
    }
}

fn edit_description(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<io::Stderr>>,
    app: &mut app::App,
    id: TaskId,
) -> Result<(), Error> {
    let original = app
        .selected_task()
        .and_then(|t| t.description.as_deref())
        .unwrap_or("")
        .to_string();

    // Leave TUI, run editor, re-enter TUI.
    restore_terminal()?;
    let result = ops::edit_in_editor(&original);
    enable_raw_mode()?;
    execute!(io::stderr(), EnterAlternateScreen, EnableMouseCapture)?;
    terminal.clear()?;

    let Some(edited) = result? else {
        return Ok(());
    };

    let edits = Edits {
        description: Some(edited),
        ..Edits::default()
    };
    ops::apply_edits(&app.store, id, &edits)?;
    app.reload()?;
    Ok(())
}

fn restore_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(io::stderr(), LeaveAlternateScreen, DisableMouseCapture)?;
    Ok(())
}
