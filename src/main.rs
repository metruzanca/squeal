use std::io::{self, stdout, Stdout};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};
use clap::Parser;

mod app;
mod ui;
mod test_db;

use app::App;
use ui::draw;

#[derive(Parser)]
#[command(name = "squeal")]
#[command(about = "A TUI SQLite database viewer")]
struct Cli {
    /// Path to the SQLite database file
    path: Option<String>,

    /// Start with an in-memory demo database
    #[arg(long)]
    demo: bool,
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();

    if cli.demo && cli.path.is_some() {
        eprintln!("Error: cannot use --demo with a database path");
        std::process::exit(1);
    }

    let mut app = if cli.demo {
        let conn = test_db::TestDb::in_memory_simple();
        match App::from_connection(conn) {
            Ok(app) => app,
            Err(e) => {
                eprintln!("Error creating demo database: {}", e);
                std::process::exit(1);
            }
        }
    } else if let Some(path) = cli.path {
        match App::new(&path) {
            Ok(app) => app,
            Err(e) => {
                eprintln!("Error opening database: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        eprintln!("Usage: squeal <sqlite-file> or squeal --demo");
        std::process::exit(1);
    };

    let mut terminal = setup_terminal()?;
    let result = run(&mut terminal, &mut app);
    restore_terminal(&mut terminal)?;
    result
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, app))?;

        if let Event::Key(key) = event::read()? && key.kind == KeyEventKind::Press {
            if key.code == KeyCode::Char('q') {
                break;
            } else if key.code == KeyCode::Esc {
                app.unfocus_table();
            } else if app.table_focused {
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => app.scroll_table_down(),
                    KeyCode::Char('k') | KeyCode::Up => app.scroll_table_up(),
                    KeyCode::Char('h') | KeyCode::Left => app.h_scroll_left(),
                    KeyCode::Char('l') | KeyCode::Right => app.h_scroll_right(),
                    _ => {}
                }
            } else {
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => app.next(),
                    KeyCode::Char('k') | KeyCode::Up => app.previous(),
                    KeyCode::Char('l') | KeyCode::Right => app.focus_table(),
                    _ => {}
                }
            }
        }
    }
    Ok(())
}
