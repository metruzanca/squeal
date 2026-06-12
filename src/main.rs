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
    widgets::{Block, Borders, Cell, List, ListItem, Row, Table, TableState},
    Frame, Terminal,
};
use rusqlite::{Connection, Result as SqliteResult};
use clap::Parser;

mod test_db;

const MAX_COL_WIDTH: u16 = 30;

struct App {
    tables: Vec<String>,
    selected: usize,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    conn: Connection,
    h_scroll: usize,
    table_state: TableState,
    table_focused: bool,
    needs_h_scroll: bool,
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
            h_scroll: 0,
            table_state: TableState::new(),
            table_focused: false,
            needs_h_scroll: false,
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
        self.h_scroll = 0;
        if self.table_focused && !self.rows.is_empty() {
            self.table_state = TableState::new().with_selected(Some(0));
        } else {
            self.table_state = TableState::new();
        }

        Ok(())
    }

    fn next(&mut self) {
        if !self.tables.is_empty() && !self.table_focused {
            self.selected = (self.selected + 1) % self.tables.len();
            let _ = self.load_table(self.selected);
        }
    }

    fn previous(&mut self) {
        if !self.tables.is_empty() && !self.table_focused {
            self.selected = if self.selected == 0 {
                self.tables.len() - 1
            } else {
                self.selected - 1
            };
            let _ = self.load_table(self.selected);
        }
    }

    fn focus_table(&mut self) {
        if !self.headers.is_empty() {
            self.table_focused = true;
            if !self.rows.is_empty() {
                self.table_state.select(Some(0));
            }
        }
    }

    fn unfocus_table(&mut self) {
        self.table_focused = false;
        self.table_state = TableState::new();
        self.h_scroll = 0;
    }

    fn scroll_table_down(&mut self) {
        if let Some(selected) = self.table_state.selected() {
            let next = (selected + 1).min(self.rows.len().saturating_sub(1));
            self.table_state.select(Some(next));
        }
    }

    fn scroll_table_up(&mut self) {
        if let Some(selected) = self.table_state.selected() {
            let prev = selected.saturating_sub(1);
            self.table_state.select(Some(prev));
        }
    }

    fn h_scroll_left(&mut self) {
        if self.table_focused && self.needs_h_scroll && self.h_scroll > 0 {
            self.h_scroll -= 1;
        }
    }

    fn h_scroll_right(&mut self) {
        if self.table_focused && self.needs_h_scroll && self.h_scroll + 1 < self.headers.len() {
            self.h_scroll += 1;
        }
    }
}

fn truncate_with_ellipsis(s: &str, max_width: usize) -> String {
    let len = s.chars().count();
    if len <= max_width {
        s.to_string()
    } else if max_width <= 3 {
        s.chars().take(max_width).collect()
    } else {
        let mut result = String::new();
        for ch in s.chars().take(max_width - 3) {
            result.push(ch);
        }
        result.push_str("...");
        result
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

fn draw(frame: &mut Frame, app: &mut App) {
    let max_table_name_len = app
        .tables
        .iter()
        .map(|t| t.chars().count() as u16)
        .max()
        .unwrap_or(0);
    let left_width = (max_table_name_len + 3).max(8); // +1 padding + 2 borders, min 8 for "Tables"

    let main_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(left_width), Constraint::Fill(1)])
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
        let inner_width = main_layout[1].width.saturating_sub(2); // -2 for borders

        // Compute column widths based on data
        let mut col_widths: Vec<u16> =
            app.headers.iter().map(|h| h.chars().count() as u16).collect();
        for row in &app.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_widths.len() {
                    col_widths[i] = col_widths[i].max(cell.chars().count() as u16);
                }
            }
        }
        // Clamp to max width
        for w in &mut col_widths {
            *w = (*w).min(MAX_COL_WIDTH);
        }

        // Determine visible columns based on h_scroll and available width
        let spacing = 1; // default column spacing
        let total_table_width = col_widths.iter().copied().sum::<u16>()
            + spacing * (app.headers.len().saturating_sub(1) as u16);
        app.needs_h_scroll = total_table_width > inner_width;

        let mut visible_count = 0;
        let mut current_width = 0;
        for i in app.h_scroll..app.headers.len() {
            if i > app.h_scroll {
                current_width += spacing;
            }
            current_width += col_widths[i];
            if current_width > inner_width && visible_count > 0 {
                break;
            }
            visible_count += 1;
        }
        visible_count = visible_count.max(1);
        let end_col = (app.h_scroll + visible_count).min(app.headers.len());

        let visible_headers = &app.headers[app.h_scroll..end_col];
        let visible_widths = &col_widths[app.h_scroll..end_col];

        let header_cells: Vec<Cell> = visible_headers
            .iter()
            .enumerate()
            .map(|(i, h)| {
                let width = visible_widths[i] as usize;
                let truncated = truncate_with_ellipsis(h, width);
                Cell::from(truncated)
                    .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
            })
            .collect();
        let header =
            Row::new(header_cells).style(Style::default().add_modifier(Modifier::UNDERLINED));

        let rows: Vec<Row> = app
            .rows
            .iter()
            .map(|row_data| {
                let visible_cells = &row_data[app.h_scroll..end_col];
                let cells: Vec<Cell> = visible_cells
                    .iter()
                    .enumerate()
                    .map(|(i, text)| {
                        let width = visible_widths[i] as usize;
                        let truncated = truncate_with_ellipsis(text, width);
                        Cell::from(truncated)
                    })
                    .collect();
                Row::new(cells)
            })
            .collect();

        let constraints: Vec<Constraint> =
            visible_widths.iter().map(|&w| Constraint::Length(w)).collect();

        let title = if app.headers.len() > visible_count {
            format!(
                "Table: {} (cols {}-{} of {})",
                app.tables[app.selected],
                app.h_scroll + 1,
                end_col,
                app.headers.len()
            )
        } else {
            format!("Table: {}", app.tables[app.selected])
        };

        let mut block = Block::default()
            .title(title)
            .borders(Borders::ALL);
        if app.table_focused {
            block = block
                .border_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
        }

        let table = Table::new(rows, &constraints)
            .header(header)
            .block(block);
        let table = if app.table_focused {
            table.highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        } else {
            table
        };
        frame.render_stateful_widget(table, main_layout[1], &mut app.table_state);
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

    #[test]
    fn test_truncate_with_ellipsis() {
        assert_eq!(truncate_with_ellipsis("hello", 10), "hello");
        assert_eq!(truncate_with_ellipsis("hello world", 5), "he...");
        assert_eq!(truncate_with_ellipsis("abc", 2), "ab");
        assert_eq!(truncate_with_ellipsis("abc", 3), "abc");
        assert_eq!(truncate_with_ellipsis("abcd", 3), "abc");
        assert_eq!(truncate_with_ellipsis("", 5), "");
    }

    #[test]
    fn test_focus_and_unfocus() {
        let path = "/tmp/squeal_test_focus.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(path).unwrap();
        assert!(!app.table_focused);
        assert_eq!(app.table_state.selected(), None);

        app.focus_table();
        assert!(app.table_focused);
        assert_eq!(app.table_state.selected(), Some(0));

        app.scroll_table_down();
        assert_eq!(app.table_state.selected(), Some(1));

        app.scroll_table_up();
        assert_eq!(app.table_state.selected(), Some(0));

        // Horizontal scrolling is blocked when needs_h_scroll is false
        app.h_scroll_right();
        assert_eq!(app.h_scroll, 0);

        app.needs_h_scroll = true;
        app.h_scroll_right();
        assert_eq!(app.h_scroll, 1);

        app.h_scroll_left();
        assert_eq!(app.h_scroll, 0);

        app.unfocus_table();
        assert!(!app.table_focused);
        assert_eq!(app.table_state.selected(), None);
        assert_eq!(app.h_scroll, 0);
    }

    #[test]
    fn test_navigation_blocked_when_focused() {
        let path = "/tmp/squeal_test_nav_block.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(path).unwrap();
        app.focus_table();
        assert!(app.table_focused);
        assert_eq!(app.selected, 0);

        app.next();
        assert_eq!(app.selected, 0); // should not change
        app.previous();
        assert_eq!(app.selected, 0); // should not change
    }
}
