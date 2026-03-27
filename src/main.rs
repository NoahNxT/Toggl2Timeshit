use std::error::Error;
use std::time::Duration;

use crossterm::ExecutableCommand;
use crossterm::event::{self, Event};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

mod app;
mod dates;
mod grouping;
mod models;
mod rollups;
mod rounding;
mod storage;
mod theme;
mod theme_studio;
mod toggl;
mod ui;
mod update;

use app::{App, AppCommand};
use dates::DateRange;
use theme_studio::ThemeStudioExit;

fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("timeshit {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--theme-studio") {
        match theme_studio::run()? {
            ThemeStudioExit::Closed => {}
            ThemeStudioExit::TimedOut => {
                println!("Theme studio timed out after 15 minutes of inactivity.");
            }
        }
        return Ok(());
    }

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

        if app.needs_refresh {
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

        if let Some(command) = app.take_pending_command() {
            match command {
                AppCommand::OpenThemeStudio => {
                    match run_theme_studio_session(&mut terminal, &mut app) {
                        Ok(ThemeStudioExit::Closed) => {}
                        Ok(ThemeStudioExit::TimedOut) => {
                            app.status = Some(
                                "Theme studio timed out after 15 minutes of inactivity."
                                    .to_string(),
                            );
                        }
                        Err(err) => {
                            app.status = Some(format!("Theme studio failed: {err}"));
                        }
                    }
                }
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

fn run_theme_studio_session(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
) -> Result<ThemeStudioExit, Box<dyn Error>> {
    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    let studio_result = theme_studio::run();

    enable_raw_mode()?;
    terminal.backend_mut().execute(EnterAlternateScreen)?;
    terminal.clear()?;
    terminal.hide_cursor()?;

    app.reload_theme_state();
    studio_result.map_err(|err| err.into())
}
