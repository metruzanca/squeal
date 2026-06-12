use ratatui::widgets::TableState;
use rusqlite::{Connection, Result as SqliteResult};

pub struct App {
    pub tables: Vec<String>,
    pub selected: usize,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub conn: Connection,
    pub h_scroll: usize,
    pub table_state: TableState,
    pub table_focused: bool,
    pub needs_h_scroll: bool,
}

impl App {
    pub fn new(db_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let conn = Connection::open(db_path)?;
        Self::from_connection(conn)
    }

    pub fn from_connection(conn: Connection) -> Result<Self, Box<dyn std::error::Error>> {
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

    pub fn load_table(&mut self, index: usize) -> Result<(), Box<dyn std::error::Error>> {
        if index >= self.tables.len() {
            return Ok(());
        }
        let table_name = &self.tables[index];

        let headers = {
            let mut stmt = self
                .conn
                .prepare(&format!("PRAGMA table_info(\"{}\")", table_name))?;
            stmt.query_map([], |row| row.get::<_, String>(1))?
                .collect::<SqliteResult<Vec<String>>>()?
        };

        let col_count = headers.len();

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

    pub fn next(&mut self) {
        if !self.tables.is_empty() && !self.table_focused {
            self.selected = (self.selected + 1) % self.tables.len();
            let _ = self.load_table(self.selected);
        }
    }

    pub fn previous(&mut self) {
        if !self.tables.is_empty() && !self.table_focused {
            self.selected = if self.selected == 0 {
                self.tables.len() - 1
            } else {
                self.selected - 1
            };
            let _ = self.load_table(self.selected);
        }
    }

    pub fn focus_table(&mut self) {
        if !self.headers.is_empty() {
            self.table_focused = true;
            if !self.rows.is_empty() {
                self.table_state.select(Some(0));
            }
        }
    }

    pub fn unfocus_table(&mut self) {
        self.table_focused = false;
        self.table_state = TableState::new();
        self.h_scroll = 0;
    }

    pub fn scroll_table_down(&mut self) {
        if let Some(selected) = self.table_state.selected() {
            let next = (selected + 1).min(self.rows.len().saturating_sub(1));
            self.table_state.select(Some(next));
        }
    }

    pub fn scroll_table_up(&mut self) {
        if let Some(selected) = self.table_state.selected() {
            let prev = selected.saturating_sub(1);
            self.table_state.select(Some(prev));
        }
    }

    pub fn h_scroll_left(&mut self) {
        if self.table_focused && self.needs_h_scroll && self.h_scroll > 0 {
            self.h_scroll -= 1;
        }
    }

    pub fn h_scroll_right(&mut self) {
        if self.table_focused && self.needs_h_scroll && self.h_scroll + 1 < self.headers.len() {
            self.h_scroll += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_db;

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
