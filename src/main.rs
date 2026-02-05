use std::error::Error;
use std::time::Duration;

use crossterm::event::{self, Event};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

mod app;
mod dates;
mod grouping;
mod models;
mod rollups;
mod rounding;
mod storage;
mod toggl;
mod ui;
mod update;

use app::App;
use dates::DateRange;

fn main() -> Result<(), Box<dyn Error>> {
    let date_range = DateRange::today();
    let force_login = false;

    let mut stdout = std::io::stdout();
    enable_raw_mode()?;
    stdout.execute(EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let needs_update_check = update::should_check_updates();
    let mut app = App::new(date_range, force_login, needs_update_check);

    loop {
        if app.needs_update_check() {
            app.check_for_update();
        }

        terminal.draw(|frame| ui::draw(frame, &mut app))?;

        if app.needs_update_install() {
            app.perform_update();
        }

        if app.needs_refresh && !app.is_update_blocking() {
            app.refresh_data();
        }

        if app.should_quit {
            break;
        }

        if event::poll(Duration::from_millis(120))? {
            let event = event::read()?;
            if let Event::Key(key) = event {
                app.handle_key_event(key);
            }
        }
    }

    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Some(message) = app.take_exit_message() {
        println!("{message}");
    }

    Ok(())
}
