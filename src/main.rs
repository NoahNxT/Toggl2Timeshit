use std::error::Error;
use std::time::Duration;

use clap::{Args, Parser, Subcommand};
use crossterm::event::{self, Event};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

mod app;
mod dates;
mod grouping;
mod models;
mod rounding;
mod storage;
mod toggl;
mod ui;
mod update;

use app::App;
use dates::{parse_date, DateRange};

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[command(flatten)]
    dates: DateArgs,
}

#[derive(Args, Debug, Default)]
struct DateArgs {
    #[arg(short = 'd', long = "date", global = true)]
    date: Option<String>,
    #[arg(short = 's', long = "start-date", global = true)]
    start_date: Option<String>,
    #[arg(short = 'e', long = "end-date", global = true)]
    end_date: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Login,
    List,
}

fn build_date_range(args: &DateArgs) -> Result<DateRange, String> {
    let date = match &args.date {
        Some(value) => Some(parse_date(value)?),
        None => None,
    };
    let start_date = match &args.start_date {
        Some(value) => Some(parse_date(value)?),
        None => None,
    };
    let end_date = match &args.end_date {
        Some(value) => Some(parse_date(value)?),
        None => None,
    };

    if date.is_none() && start_date.is_none() && end_date.is_some() {
        return Err("End date requires a start date.".to_string());
    }

    DateRange::from_options(date, start_date, end_date)
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let date_range = match build_date_range(&cli.dates) {
        Ok(range) => range,
        Err(err) => {
            eprintln!("{err}");
            return Ok(());
        }
    };
    let force_login = matches!(cli.command, Some(Commands::Login));

    let mut stdout = std::io::stdout();
    enable_raw_mode()?;
    stdout.execute(EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let needs_update_check = cli.command.is_none() && update::should_check_updates();
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
