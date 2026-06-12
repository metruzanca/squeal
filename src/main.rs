//! Entry point for the `squeal` TUI application.
//!
//! This file sets up the terminal (raw mode + alternate screen), parses CLI arguments, wires
//! together the [`App`] state and the [`ui`] renderer, and runs the main input loop. It maps
//! keyboard events to application commands (table selection, focus/unfocus, scrolling, and quit).

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

use app::{App, FilterMode};
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

    let mut app = build_app(&cli);

    let mut terminal = setup_terminal()?;
    let result = run(&mut terminal, &mut app);
    restore_terminal(&mut terminal)?;
    result
}

fn build_app(cli: &Cli) -> App {
    if cli.demo && cli.path.is_some() {
        eprintln!("Error: cannot use --demo with a database path");
        std::process::exit(1);
    }

    if cli.demo {
        let conn = test_db::TestDb::in_memory_demo();
        match App::from_connection(conn) {
            Ok(app) => app,
            Err(e) => {
                eprintln!("Error creating demo database: {}", e);
                std::process::exit(1);
            }
        }
    } else if let Some(path) = cli.path.as_deref() {
        match App::new(path) {
            Ok(app) => app,
            Err(e) => {
                eprintln!("Error opening database: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        eprintln!("Usage: squeal <sqlite-file> or squeal --demo");
        std::process::exit(1);
    }
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
            } else if app.help_open {
                if key.code == KeyCode::Char('?') || key.code == KeyCode::Esc {
                    app.close_help();
                }
            } else if app.modal_open {
                match key.code {
                    KeyCode::Esc => app.close_modal(),
                    KeyCode::Char('j') | KeyCode::Down => app.modal_scroll_down(),
                    KeyCode::Char('k') | KeyCode::Up => app.modal_scroll_up(),
                    KeyCode::Char('h') | KeyCode::Left => app.modal_h_scroll_left(),
                    KeyCode::Char('l') | KeyCode::Right => app.modal_h_scroll_right(),
                    KeyCode::Enter => app.modal_select_table(),
                    _ => {}
                }
            } else if key.code == KeyCode::Esc {
                if app.filter_mode != FilterMode::None {
                    app.cancel_filter_mode();
                } else if app.query_edit_mode {
                    app.query_edit_mode = false;
                    let _ = app.run_query();
                    app.save_current_query();
                } else if app.rename_mode {
                    app.cancel_rename();
                } else {
                    app.unfocus_table();
                }
            } else if key.code == KeyCode::Char('?') {
                app.toggle_help();
            } else if app.table_focused {
                if app.filter_mode != FilterMode::None {
                    // Filter mode: shared between query view and regular tables
                    match app.filter_mode {
                        FilterMode::HeaderSelect => {
                            match key.code {
                                KeyCode::Char('/') => app.cancel_filter_mode(),
                                KeyCode::Char('h') | KeyCode::Left => app.move_filter_col_left(),
                                KeyCode::Char('l') | KeyCode::Right => app.move_filter_col_right(),
                                KeyCode::Char('k') | KeyCode::Up => app.cycle_sort_order(),
                                KeyCode::Char('j') | KeyCode::Down => app.cycle_sort_order(),
                                KeyCode::Enter => app.enter_filter_for_col(),
                                KeyCode::Delete => app.delete_current_filter(),
                                _ => {}
                            }
                        }
                        FilterMode::TypeSelect => {
                            match key.code {
                                KeyCode::Char('h') | KeyCode::Left => app.toggle_filter_type(),
                                KeyCode::Char('l') | KeyCode::Right => app.toggle_filter_type(),
                                KeyCode::Char('j') | KeyCode::Down => app.toggle_filter_type(),
                                KeyCode::Char('k') | KeyCode::Up => app.toggle_filter_type(),
                                KeyCode::Enter => app.move_to_value_input(),
                                KeyCode::Esc => app.cancel_filter_mode(),
                                KeyCode::Delete => app.delete_current_filter(),
                                _ => {}
                            }
                        }
                        FilterMode::ValueInput => {
                            match key.code {
                                KeyCode::Char(c) => app.filter_input_char(c),
                                KeyCode::Backspace => app.filter_input_backspace(),
                                KeyCode::Enter => app.apply_filter(),
                                KeyCode::Esc => app.cancel_filter_mode(),
                                KeyCode::Delete => app.delete_current_filter(),
                                _ => {}
                            }
                        }
                        FilterMode::None => unreachable!(),
                    }
                } else if app.is_query_view {
                    if app.rename_mode {
                        match key.code {
                            KeyCode::Char(c) => app.rename_value.push(c),
                            KeyCode::Backspace => { app.rename_value.pop(); }
                            KeyCode::Enter => app.apply_rename(),
                            KeyCode::Esc => app.cancel_rename(),
                            _ => {}
                        }
                    } else if app.query_edit_mode {
                        match key.code {
                            KeyCode::Esc | KeyCode::Tab => {
                                app.query_edit_mode = false;
                                let _ = app.run_query();
                                app.save_current_query();
                            }
                            KeyCode::Char(c) => app.insert_query_char(c),
                            KeyCode::Backspace => app.backspace_query_char(),
                            KeyCode::Delete => app.delete_query_char(),
                            KeyCode::Enter => {
                                if key.modifiers.contains(event::KeyModifiers::CONTROL) {
                                    let _ = app.run_query();
                                    app.save_current_query();
                                } else {
                                    app.insert_query_char('\n');
                                }
                            }
                            KeyCode::Left => app.move_query_cursor_left(),
                            KeyCode::Right => app.move_query_cursor_right(),
                            KeyCode::Up => app.move_query_cursor_up(),
                            KeyCode::Down => app.move_query_cursor_down(),
                            KeyCode::Home => app.move_query_cursor_home(),
                            KeyCode::End => app.move_query_cursor_end(),
                            _ => {}
                        }
                    } else {
                        match key.code {
                            KeyCode::Char('/') => app.toggle_filter_mode(),
                            KeyCode::Char('r') => app.start_rename(),
                            KeyCode::Tab => app.query_edit_mode = true,
                            KeyCode::Char('j') | KeyCode::Down => app.scroll_table_down(),
                            KeyCode::Char('k') | KeyCode::Up => app.scroll_table_up(),
                            KeyCode::Char('h') | KeyCode::Left => app.h_scroll_left(),
                            KeyCode::Char('l') | KeyCode::Right => app.h_scroll_right(),
                            KeyCode::PageDown => app.page_down(),
                            KeyCode::PageUp => app.page_up(),
                            KeyCode::Enter => { let _ = app.open_modal(); }
                            _ => {}
                        }
                    }
                } else {
                    // Regular table view
                    match key.code {
                        KeyCode::Char('/') => app.toggle_filter_mode(),
                        KeyCode::Tab => app.unfocus_table(),
                        KeyCode::Char('j') | KeyCode::Down => app.scroll_table_down(),
                        KeyCode::Char('k') | KeyCode::Up => app.scroll_table_up(),
                        KeyCode::Char('h') | KeyCode::Left => app.h_scroll_left(),
                        KeyCode::Char('l') | KeyCode::Right => app.h_scroll_right(),
                        KeyCode::PageDown => app.page_down(),
                        KeyCode::PageUp => app.page_up(),
                        KeyCode::Enter => { let _ = app.open_modal(); }
                        _ => {}
                    }
                }
            } else {
                match key.code {
                    KeyCode::Tab | KeyCode::Enter => app.focus_table(),
                    KeyCode::Char('j') | KeyCode::Down => app.next(),
                    KeyCode::Char('k') | KeyCode::Up => app.previous(),
                    KeyCode::Char('n') => {
                        app.create_new_query();
                    }
                    KeyCode::Char('D') => {
                        app.delete_current_query();
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}
