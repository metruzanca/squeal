use std::io::{self, stdout, Stdout};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, List, ListItem, Row, Table},
    Frame, Terminal,
};
use rusqlite::{Connection, Result as SqliteResult};
use clap::Parser;

mod test_db;

struct App {
    tables: Vec<String>,
    selected: usize,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    conn: Connection,
}

impl App {
    fn new(db_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let conn = Connection::open(db_path)?;
        Self::from_connection(conn)
    }

    fn from_connection(conn: Connection) -> Result<Self, Box<dyn std::error::Error>> {
        let tables = {
            let mut stmt = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")?;
            stmt.query_map([], |row| row.get(0))?
                .collect::<SqliteResult<Vec<String>>>()?
        };

        let mut app = App {
            tables,
            selected: 0,
            headers: Vec::new(),
            rows: Vec::new(),
            conn,
        };

        if !app.tables.is_empty() {
            app.load_table(0)?;
        }

        Ok(app)
    }

    fn load_table(&mut self, index: usize) -> Result<(), Box<dyn std::error::Error>> {
        if index >= self.tables.len() {
            return Ok(());
        }
        let table_name = &self.tables[index];

        // Get column names via PRAGMA
        let headers = {
            let mut stmt = self
                .conn
                .prepare(&format!("PRAGMA table_info(\"{}\")", table_name))?;
            stmt.query_map([], |row| row.get::<_, String>(1))?
                .collect::<SqliteResult<Vec<String>>>()?
        };

        let col_count = headers.len();

        // Get data
        let rows = {
            let mut stmt = self
                .conn
                .prepare(&format!("SELECT * FROM \"{}\" LIMIT 100", table_name))?;
            stmt.query_map([], |row| {
                let mut values = Vec::with_capacity(col_count);
                for i in 0..col_count {
                    let value = match row.get::<_, rusqlite::types::Value>(i)? {
                        rusqlite::types::Value::Null => String::new(),
                        rusqlite::types::Value::Integer(v) => v.to_string(),
                        rusqlite::types::Value::Real(v) => v.to_string(),
                        rusqlite::types::Value::Text(v) => v,
                        rusqlite::types::Value::Blob(v) => String::from_utf8_lossy(&v).to_string(),
                    };
                    values.push(value);
                }
                Ok(values)
            })?
            .collect::<SqliteResult<Vec<Vec<String>>>>()?
        };

        self.headers = headers;
        self.rows = rows;

        Ok(())
    }

    fn next(&mut self) {
        if !self.tables.is_empty() {
            self.selected = (self.selected + 1) % self.tables.len();
            let _ = self.load_table(self.selected);
        }
    }

    fn previous(&mut self) {
        if !self.tables.is_empty() {
            self.selected = if self.selected == 0 {
                self.tables.len() - 1
            } else {
                self.selected - 1
            };
            let _ = self.load_table(self.selected);
        }
    }
}

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
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Char('j') | KeyCode::Down => app.next(),
                KeyCode::Char('k') | KeyCode::Up => app.previous(),
                _ => {}
            }
        }
    }
    Ok(())
}

fn draw(frame: &mut Frame, app: &App) {
    let main_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
        .split(frame.size());

    // Left column: Table list
    let items: Vec<ListItem> = app
        .tables
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let style = if i == app.selected {
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(name.as_str()).style(style)
        })
        .collect();

    let list = List::new(items).block(Block::default().title("Tables").borders(Borders::ALL));
    frame.render_widget(list, main_layout[0]);

    // Right column: Table data
    if !app.headers.is_empty() {
        let header_cells: Vec<Cell> = app
            .headers
            .iter()
            .map(|h| {
                Cell::from(h.as_str())
                    .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
            })
            .collect();
        let header =
            Row::new(header_cells).style(Style::default().add_modifier(Modifier::UNDERLINED));

        let rows: Vec<Row> = app
            .rows
            .iter()
            .map(|row_data| {
                let cells: Vec<Cell> =
                    row_data.iter().map(|text| Cell::from(text.as_str())).collect();
                Row::new(cells)
            })
            .collect();

        let col_widths: Vec<Constraint> = app
            .headers
            .iter()
            .map(|_| Constraint::Percentage(100 / app.headers.len() as u16))
            .collect();

        let table = Table::new(rows, &col_widths)
            .header(header)
            .block(
                Block::default()
                    .title(format!("Table: {}", app.tables[app.selected]))
                    .borders(Borders::ALL),
            );
        frame.render_widget(table, main_layout[1]);
    } else {
        let paragraph = ratatui::widgets::Paragraph::new("No table selected or table is empty")
            .block(Block::default().title("Data").borders(Borders::ALL));
        frame.render_widget(paragraph, main_layout[1]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_new_loads_tables() {
        let path = "/tmp/squeal_test.db";
        test_db::TestDb::simple(path);
        let app = App::new(path).unwrap();
        assert_eq!(app.tables, vec!["products", "users"]);
        assert_eq!(app.headers, vec!["id", "title", "price"]);
        assert_eq!(app.rows.len(), 2);
        assert_eq!(app.rows[0], vec!["1", "Widget", "9.99"]);
    }

    #[test]
    fn test_app_navigation() {
        let path = "/tmp/squeal_test_nav.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(path).unwrap();
        assert_eq!(app.selected, 0);
        assert_eq!(app.tables[app.selected], "products");

        app.next();
        assert_eq!(app.selected, 1);
        assert_eq!(app.tables[app.selected], "users");
        assert_eq!(app.headers, vec!["id", "name", "email"]);
        assert_eq!(app.rows.len(), 3);

        app.next();
        assert_eq!(app.selected, 0);
        assert_eq!(app.tables[app.selected], "products");

        app.previous();
        assert_eq!(app.selected, 1);
        assert_eq!(app.tables[app.selected], "users");
    }

    #[test]
    fn test_app_empty_db() {
        let path = "/tmp/squeal_test_empty.db";
        test_db::TestDb::empty(path);
        let app = App::new(path).unwrap();
        assert!(app.tables.is_empty());
        assert!(app.headers.is_empty());
        assert!(app.rows.is_empty());
    }

    #[test]
    fn test_app_large_db() {
        let path = "/tmp/squeal_test_large.db";
        test_db::TestDb::large(path, 150);
        let app = App::new(path).unwrap();
        assert_eq!(app.tables, vec!["items"]);
        assert_eq!(app.headers, vec!["id", "name", "value"]);
        assert_eq!(app.rows.len(), 100); // limited to 100 rows
    }

    #[test]
    fn test_app_wide_db() {
        let path = "/tmp/squeal_test_wide.db";
        test_db::TestDb::wide(path);
        let app = App::new(path).unwrap();
        assert_eq!(app.tables, vec!["wide_table"]);
        assert_eq!(app.headers.len(), 10);
        assert_eq!(app.rows.len(), 2);
    }

    #[test]
    fn test_app_in_memory_demo() {
        let conn = test_db::TestDb::in_memory_simple();
        let app = App::from_connection(conn).unwrap();
        assert_eq!(app.tables, vec!["products", "users"]);
        assert_eq!(app.headers, vec!["id", "title", "price"]);
        assert_eq!(app.rows.len(), 2);
        assert_eq!(app.rows[0], vec!["1", "Widget", "9.99"]);
    }
}
