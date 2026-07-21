//! Entry point for the `squeal` TUI application.
//!
//! This file sets up the terminal (raw mode + alternate screen), parses CLI arguments, wires
//! together the [`App`] state and the [`ui`] renderer, and runs the main input loop. It maps
//! keyboard events to application commands (table selection, focus/unfocus, scrolling, and quit).

use std::io::{self, stdout, Stdout};
use std::time::{Duration, Instant};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};
use clap::Parser;

mod app;
mod config;
mod driver;
mod startup;
mod ui;

mod test_db;

#[cfg(test)]
mod app_tests;

use app::{App, AppState, ConnectingState, FilterMode, generate_db_name};
use config::Config;
use driver::{sqlite::SQLiteDriver, postgres::PostgresDriver};
use startup::run_startup;
use ui::draw;

#[derive(Parser)]
#[command(name = "squeal")]
#[command(about = "A TUI database viewer")]
struct Cli {
    /// Database connection string or path
    /// Auto-detects: postgres://... for PostgreSQL, otherwise SQLite
    path: Option<String>,

    /// Start with an in-memory demo SQLite database
    #[arg(long)]
    demo: bool,
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();

    let mut terminal = setup_terminal()?;
    let result = run_with_terminal(&mut terminal, &cli);
    restore_terminal(&mut terminal)?;
    result
}

fn run_with_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>, cli: &Cli) -> io::Result<()> {
    if cli.demo && cli.path.is_some() {
        restore_terminal(terminal)?;
        eprintln!("Error: cannot use --demo with a database path");
        std::process::exit(1);
    }

    if cli.demo {
        let app = build_demo_app();
        run(terminal, &mut AppState::Ready(app))
    } else if let Some(path) = cli.path.as_deref() {
        let is_postgres = path.starts_with("postgres://") || path.starts_with("postgresql://");
        save_recent(path);
        if is_postgres {
            let db_name = generate_db_name(path);
            let cs = ConnectingState::new(path.to_string(), db_name);
            run(terminal, &mut AppState::Connecting(cs))
        } else {
            let app = match build_app_from_path(path) {
                Ok(app) => app,
                Err(e) => {
                    restore_terminal(terminal)?;
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            };
            run(terminal, &mut AppState::Ready(app))
        }
    } else {
        // No arguments: show startup screen
        match run_startup(terminal)? {
            Some(path) => {
                let is_postgres = path.starts_with("postgres://") || path.starts_with("postgresql://");
                save_recent(&path);
                if is_postgres {
                    let db_name = generate_db_name(&path);
                    let cs = ConnectingState::new(path, db_name);
                    run(terminal, &mut AppState::Connecting(cs))
                } else {
                    let app = match build_app_from_path(&path) {
                        Ok(app) => app,
                        Err(e) => {
                            restore_terminal(terminal)?;
                            eprintln!("{}", e);
                            std::process::exit(1);
                        }
                    };
                    run(terminal, &mut AppState::Ready(app))
                }
            }
            None => Ok(()),
        }
    }
}

fn build_demo_app() -> App {
    let conn = test_db::TestDb::in_memory_demo();
    let driver = SQLiteDriver::from_connection(conn);
    match App::new(Box::new(driver), "demo".to_string()) {
        Ok(mut app) => {
            app.save_queries = false;
            app
        }
        Err(e) => {
            eprintln!("Error creating demo database: {}", e);
            std::process::exit(1);
        }
    }
}

fn build_app_from_path(path: &str) -> Result<App, String> {
    let is_postgres = path.starts_with("postgres://") || path.starts_with("postgresql://");
    let driver: Box<dyn driver::DbDriver> = if is_postgres {
        PostgresDriver::new(path)
            .map(|d| Box::new(d) as Box<dyn driver::DbDriver>)
            .map_err(|e| format!("Error connecting to PostgreSQL: {}", e))?
    } else {
        SQLiteDriver::new(path)
            .map(|d| Box::new(d) as Box<dyn driver::DbDriver>)
            .map_err(|e| format!("Error opening SQLite database: {}", e))?
    };

    let db_name = generate_db_name(path);
    let mut app = App::new(driver, db_name).map_err(|e| format!("Error opening database: {}", e))?;
    app.start_background_loader(path.to_string(), is_postgres);
    Ok(app)
}

fn save_recent(path: &str) {
    let conn_type = if path.starts_with("postgres://") || path.starts_with("postgresql://") {
        "postgres"
    } else {
        "sqlite"
    };
    if let Ok(mut cfg) = Config::load() {
        cfg.add_recent(path, conn_type);
        let _ = cfg.save();
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

const REFRESH_INTERVAL: Duration = Duration::from_millis(50);
const AUTO_REFRESH_INTERVAL: Duration = Duration::from_secs(5);

fn should_auto_refresh(app: &App) -> bool {
    !app.help_open
        && !app.modal_open
        && !app.fuzzy_open
        && !app.peak_open
        && app.filter_mode == app::FilterMode::None
        && !app.is_query_view
        && !app.headers.is_empty()
        && !app.query_edit_mode
        && !app.rename_mode
}

fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>, state: &mut AppState) -> io::Result<()> {
    let mut last_auto_refresh = Instant::now();
    loop {
        terminal.draw(|frame| draw(frame, state))?;

        match state {
            AppState::Connecting(cs) => {
                if let Some(result) = cs.poll() {
                    match result {
                        Ok(driver) => {
                            let path = std::mem::take(&mut cs.path);
                            let db_name = std::mem::take(&mut cs.db_name);
                            match App::new(driver, db_name) {
                                Ok(mut app) => {
                                    let is_postgres =
                                        path.starts_with("postgres://") || path.starts_with("postgresql://");
                                    app.start_background_loader(path, is_postgres);
                                    *state = AppState::Ready(app);
                                    continue;
                                }
                                Err(e) => {
                                    cs.error = Some(format!("Error loading tables: {}", e));
                                }
                            }
                        }
                        Err(e) => {
                            cs.error = Some(e);
                        }
                    }
                }

                if event::poll(REFRESH_INTERVAL)? {
                    if let Event::Key(key) = event::read()? && key.kind == KeyEventKind::Press {
                        if key.code == KeyCode::Char('q') {
                            break;
                        }
                    }
                }
            }
            AppState::Ready(app) => {
                app.check_bg_results();

                if event::poll(REFRESH_INTERVAL)? {
                    if let Event::Key(key) = event::read()? && key.kind == KeyEventKind::Press {
                    if app.fuzzy_open {
                        match key.code {
                            KeyCode::Backspace => app.fuzzy_input_backspace(),
                            KeyCode::Enter => { let _ = app.fuzzy_select(); }
                            KeyCode::Esc => app.close_fuzzy(),
                            KeyCode::Down => app.fuzzy_next(),
                            KeyCode::Up => app.fuzzy_previous(),
                            KeyCode::Char(c) => app.fuzzy_input_char(c),
                            _ => {}
                        }
                    } else if key.code == KeyCode::Char('q') {
                        break;
                    } else if (key.code == KeyCode::Char('p') || key.code == KeyCode::Char('k'))
                        && key.modifiers.contains(KeyModifiers::CONTROL) {
                        app.toggle_fuzzy();
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
                    } else if app.peak_open {
                        match key.code {
                            KeyCode::Esc => app.close_peak(),
                            KeyCode::Char('j') | KeyCode::Down => app.peak_scroll_down(),
                            KeyCode::Char('k') | KeyCode::Up => app.peak_scroll_up(),
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
                                        KeyCode::Char('h') | KeyCode::Left => app.toggle_filter_type_back(),
                                        KeyCode::Char('l') | KeyCode::Right => app.toggle_filter_type(),
                                        KeyCode::Char('j') | KeyCode::Down => app.toggle_filter_type_back(),
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
                                    KeyCode::Char(' ') => app.open_peak(),
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
                                KeyCode::Char(' ') => app.open_peak(),
                                KeyCode::Char('r') => { let _ = app.refresh_current_table(); }
                                _ => {}
                            }
                        }
                    } else {
                        match key.code {
                            KeyCode::Tab | KeyCode::Enter => app.focus_table(),
                            KeyCode::Char('j') | KeyCode::Down => {
                                app.next();
                                // Drain additional buffered sidebar nav events
                                while event::poll(std::time::Duration::ZERO).unwrap_or(false) {
                                    if let Event::Key(k) = event::read().unwrap_or(Event::Key(KeyCode::Null.into()))
                                        && k.kind == KeyEventKind::Press
                                    {
                                        match k.code {
                                            KeyCode::Char('j') | KeyCode::Down => app.next(),
                                            KeyCode::Char('k') | KeyCode::Up => app.previous(),
                                            _ => break,
                                        }
                                    } else {
                                        break;
                                    }
                                }
                                let _ = app.process_sidebar_load();
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                app.previous();
                                // Drain additional buffered sidebar nav events
                                while event::poll(std::time::Duration::ZERO).unwrap_or(false) {
                                    if let Event::Key(k) = event::read().unwrap_or(Event::Key(KeyCode::Null.into()))
                                        && k.kind == KeyEventKind::Press
                                    {
                                        match k.code {
                                            KeyCode::Char('j') | KeyCode::Down => app.next(),
                                            KeyCode::Char('k') | KeyCode::Up => app.previous(),
                                            _ => break,
                                        }
                                    } else {
                                        break;
                                    }
                                }
                                let _ = app.process_sidebar_load();
                            }
                            KeyCode::Left => {
                                if app.is_on_group_header() {
                                    if let Some(gi) = app.current_group_index() {
                                        // Only collapse if currently expanded
                                        if app.groups.get(gi).is_some_and(|g| g.expanded) {
                                            app.toggle_group(gi);
                                        }
                                    }
                                }
                            }
                            KeyCode::Right => {
                                if app.is_on_group_header() {
                                    if let Some(gi) = app.current_group_index() {
                                        // Only expand if currently collapsed
                                        if app.groups.get(gi).is_some_and(|g| !g.expanded) {
                                            app.toggle_group(gi);
                                        }
                                    }
                                }
                            }
                            KeyCode::Char('n') => {
                                app.create_new_query();
                            }
                            KeyCode::Char('D') => {
                                app.delete_current_query();
                            }
                            _ => {}
                        }
                    }
                    }  // end inner Event::Key match
                } else if should_auto_refresh(app)
                    && app.can_auto_refresh()
                    && last_auto_refresh.elapsed() >= AUTO_REFRESH_INTERVAL
                {
                    last_auto_refresh = Instant::now();
                    let _ = app.refresh_current_table();
                }
            }
        }
    }
    Ok(())
}
