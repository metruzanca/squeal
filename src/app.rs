//! Application state and business logic.
//!
//! This module holds the [`App`] struct, which manages the SQLite connection, the list of
//! database tables, the currently loaded table data, and all navigation/focus state. It also
//! encapsulates the operations for switching tables, focusing/unfocusing the table view, and
//! scrolling both horizontally and vertically within the data panel.

use ratatui::widgets::TableState;
use rusqlite::{Connection, Result as SqliteResult};

/// Information about a single foreign key constraint on a table.
/// A composite key may have multiple `ForeignKeyInfo` rows with the same `id`.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ForeignKeyInfo {
    pub id: i32,
    pub seq: i32,
    pub table: String,
    pub from: String,
    pub to: String,
}

/// A single related record fetched for a foreign key value.
#[derive(Debug, Clone)]
pub struct RelatedRecord {
    pub table_name: String,
    pub fk_column: String,
    pub ref_column: String,
    pub fk_value: String,
    pub headers: Vec<String>,
    pub row: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FilterOp {
    Equals,
    Contains,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FilterMode {
    None,
    HeaderSelect,
    TypeSelect,
    ValueInput,
}

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
    pub has_more_rows: bool,
    pub page_size: usize,
    pub scroll_offset: usize,
    pub modal_open: bool,
    pub modal_records: Vec<RelatedRecord>,
    pub modal_selected: usize,
    pub modal_h_scroll: usize,
    pub modal_needs_h_scroll: bool,
    pub help_open: bool,
    pub filter_mode: FilterMode,
    pub filter_col: usize,
    pub filters: Vec<Option<(FilterOp, String)>>,
    pub sort_col: Option<usize>,
    pub sort_asc: bool,
    pub temp_filter_op: FilterOp,
    pub temp_filter_value: String,
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
            has_more_rows: false,
            page_size: 1,
            scroll_offset: 0,
            modal_open: false,
            modal_records: Vec::new(),
            modal_selected: 0,
            modal_h_scroll: 0,
            modal_needs_h_scroll: false,
            help_open: false,
            filter_mode: FilterMode::None,
            filter_col: 0,
            filters: Vec::new(),
            sort_col: None,
            sort_asc: true,
            temp_filter_op: FilterOp::Equals,
            temp_filter_value: String::new(),
        };

        if !app.tables.is_empty() {
            app.load_table(0)?;
        }

        Ok(app)
    }

    fn fetch_rows(
        &self,
        table_name: &str,
        offset: usize,
        limit: usize,
        col_count: usize,
    ) -> Result<Vec<Vec<String>>, Box<dyn std::error::Error>> {
        let mut sql = format!("SELECT * FROM \"{}\"", table_name);

        let active_filters: Vec<(usize, &FilterOp, &String)> = self
            .filters
            .iter()
            .enumerate()
            .filter_map(|(i, f)| f.as_ref().map(|(op, val)| (i, op, val)))
            .collect();

        if !active_filters.is_empty() {
            let mut where_clauses = Vec::new();
            for (i, op, _val) in &active_filters {
                let clause = match op {
                    FilterOp::Equals => {
                        format!("CAST(\"{}\" AS TEXT) = ?", self.headers[*i])
                    }
                    FilterOp::Contains => {
                        format!(
                            "LOWER(CAST(\"{}\" AS TEXT)) LIKE LOWER('%' || ? || '%')",
                            self.headers[*i]
                        )
                    }
                };
                where_clauses.push(clause);
            }
            sql.push_str(&format!(" WHERE {}", where_clauses.join(" AND ")));
        }

        if let Some(sort_col) = self.sort_col {
            let dir = if self.sort_asc { "ASC" } else { "DESC" };
            sql.push_str(&format!(" ORDER BY \"{}\" {}", self.headers[sort_col], dir));
        }

        sql.push_str(&format!(" LIMIT {} OFFSET {}", limit, offset));

        let mut stmt = self.conn.prepare(&sql)?;

        let params: Vec<&dyn rusqlite::types::ToSql> = active_filters
            .iter()
            .map(|(_, _, val)| *val as &dyn rusqlite::types::ToSql)
            .collect();

        let rows = stmt
            .query_map(&params[..], |row| {
                let mut values = Vec::with_capacity(col_count);
                for i in 0..col_count {
                    let value = match row.get::<_, rusqlite::types::Value>(i)? {
                        rusqlite::types::Value::Null => String::new(),
                        rusqlite::types::Value::Integer(v) => v.to_string(),
                        rusqlite::types::Value::Real(v) => v.to_string(),
                        rusqlite::types::Value::Text(v) => v,
                        rusqlite::types::Value::Blob(v) => {
                            String::from_utf8_lossy(&v).to_string()
                        }
                    };
                    values.push(value);
                }
                Ok(values)
            })?
            .collect::<SqliteResult<Vec<Vec<String>>>>()?;
        Ok(rows)
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

        // Set headers and reset filter state before fetching
        self.headers = headers;
        self.filters = vec![None; self.headers.len()];
        self.filter_mode = FilterMode::None;
        self.filter_col = 0;
        self.sort_col = None;
        self.sort_asc = true;
        self.temp_filter_op = FilterOp::Equals;
        self.temp_filter_value = String::new();

        let rows = self.fetch_rows(table_name, 0, 100, col_count)?;

        self.rows = rows;
        self.has_more_rows = self.rows.len() == 100;
        self.h_scroll = 0;
        self.scroll_offset = 0;
        self.close_modal();
        if self.table_focused && !self.rows.is_empty() {
            self.table_state = TableState::new().with_selected(Some(0));
        } else {
            self.table_state = TableState::new();
        }

        Ok(())
    }

    pub fn fetch_more_rows(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.tables.is_empty() || !self.has_more_rows {
            return Ok(());
        }
        let table_name = &self.tables[self.selected];
        let col_count = self.headers.len();
        let offset = self.rows.len();

        let new_rows = self.fetch_rows(table_name, offset, 100, col_count)?;
        self.has_more_rows = new_rows.len() == 100;
        self.rows.extend(new_rows);
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
            self.scroll_offset = 0;
            if !self.rows.is_empty() {
                self.table_state.select(Some(0));
            }
        }
    }

    pub fn unfocus_table(&mut self) {
        self.table_focused = false;
        self.table_state = TableState::new();
        self.h_scroll = 0;
        self.close_modal();
        self.filter_mode = FilterMode::None;
    }

    fn ensure_cursor_visible(&mut self) {
        if let Some(selected) = self.table_state.selected() {
            if self.page_size == 0 {
                return;
            }
            if selected >= self.scroll_offset + self.page_size {
                self.scroll_offset = selected.saturating_sub(self.page_size - 1);
            } else if selected < self.scroll_offset {
                self.scroll_offset = selected;
            }
        }
    }

    pub fn scroll_table_down(&mut self) {
        if let Some(selected) = self.table_state.selected() {
            if selected + 1 >= self.rows.len() && self.has_more_rows {
                let _ = self.fetch_more_rows();
            }
            let next = (selected + 1).min(self.rows.len().saturating_sub(1));
            self.table_state.select(Some(next));
            self.ensure_cursor_visible();
        }
    }

    pub fn scroll_table_up(&mut self) {
        if let Some(selected) = self.table_state.selected() {
            let prev = selected.saturating_sub(1);
            self.table_state.select(Some(prev));
            self.ensure_cursor_visible();
        }
    }

    pub fn page_down(&mut self) {
        if self.page_size == 0 {
            return;
        }
        let max_offset = if self.rows.len() == 0 {
            0
        } else {
            ((self.rows.len() - 1) / self.page_size) * self.page_size
        };
        if self.scroll_offset >= max_offset {
            return;
        }
        let target = self.scroll_offset + self.page_size;
        while self.rows.len() <= target + self.page_size && self.has_more_rows {
            let _ = self.fetch_more_rows();
        }
        let max_offset = if self.rows.len() == 0 {
            0
        } else {
            ((self.rows.len() - 1) / self.page_size) * self.page_size
        };
        let new_offset = target.min(max_offset);
        let end = self.rows.len().saturating_sub(1);
        if let Some(selected) = self.table_state.selected() {
            let visual_pos = selected.saturating_sub(self.scroll_offset);
            let new_selected = (new_offset + visual_pos).min(end);
            self.table_state.select(Some(new_selected));
        }
        self.scroll_offset = new_offset;
    }

    pub fn page_up(&mut self) {
        if self.page_size == 0 || self.scroll_offset == 0 {
            return;
        }
        let new_offset = self.scroll_offset.saturating_sub(self.page_size);
        let end = self.rows.len().saturating_sub(1);
        if let Some(selected) = self.table_state.selected() {
            let visual_pos = selected.saturating_sub(self.scroll_offset);
            let new_selected = (new_offset + visual_pos).min(end);
            self.table_state.select(Some(new_selected));
        }
        self.scroll_offset = new_offset;
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

    fn get_foreign_keys(
        &self,
        table_name: &str,
    ) -> Result<Vec<ForeignKeyInfo>, Box<dyn std::error::Error>> {
        let mut stmt = self
            .conn
            .prepare(&format!("PRAGMA foreign_key_list(\"{}\")", table_name))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ForeignKeyInfo {
                    id: row.get(0)?,
                    seq: row.get(1)?,
                    table: row.get(2)?,
                    from: row.get(3)?,
                    to: row.get(4)?,
                })
            })?
            .collect::<SqliteResult<Vec<ForeignKeyInfo>>>()?;
        Ok(rows)
    }

    pub fn open_modal(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(selected) = self.table_state.selected() else {
            return Ok(());
        };
        if selected >= self.rows.len() {
            return Ok(());
        }
        let table_name = &self.tables[self.selected];
        let fks = self.get_foreign_keys(table_name)?;
        if fks.is_empty() {
            return Ok(());
        }
        let row = &self.rows[selected];
        let mut records = Vec::new();
        for fk in fks {
            let col_idx = self.headers.iter().position(|h| h == &fk.from);
            let Some(idx) = col_idx else { continue };
            let fk_value = &row[idx];
            if fk_value.is_empty() {
                continue;
            }
            // Fetch the referenced table's headers
            let ref_headers: Vec<String> = {
                let mut stmt = self
                    .conn
                    .prepare(&format!("PRAGMA table_info(\"{}\")", fk.table))?;
                stmt.query_map([], |row| row.get::<_, String>(1))?
                    .collect::<SqliteResult<Vec<String>>>()?
            };
            // Fetch the referenced row
            let query = format!(
                "SELECT * FROM \"{}\" WHERE \"{}\" = ? LIMIT 1",
                fk.table, fk.to
            );
            let mut stmt = self.conn.prepare(&query)?;
            let col_count = ref_headers.len();
            let mut rows = stmt.query_map([fk_value.clone()], |r| {
                let mut values = Vec::with_capacity(col_count);
                for i in 0..col_count {
                    let value = match r.get::<_, rusqlite::types::Value>(i)? {
                        rusqlite::types::Value::Null => String::new(),
                        rusqlite::types::Value::Integer(v) => v.to_string(),
                        rusqlite::types::Value::Real(v) => v.to_string(),
                        rusqlite::types::Value::Text(v) => v,
                        rusqlite::types::Value::Blob(v) => {
                            String::from_utf8_lossy(&v).to_string()
                        }
                    };
                    values.push(value);
                }
                Ok(values)
            })?;
            if let Some(Ok(related_row)) = rows.next() {
                records.push(RelatedRecord {
                    table_name: fk.table.clone(),
                    fk_column: fk.from.clone(),
                    ref_column: fk.to.clone(),
                    fk_value: fk_value.clone(),
                    headers: ref_headers,
                    row: related_row,
                });
            }
        }
        self.modal_records = records;
        self.modal_open = true;
        self.modal_selected = 0;
        self.modal_h_scroll = 0;
        self.modal_needs_h_scroll = false;
        Ok(())
    }

    pub fn close_modal(&mut self) {
        self.modal_open = false;
        self.modal_records.clear();
        self.modal_selected = 0;
        self.modal_h_scroll = 0;
        self.modal_needs_h_scroll = false;
    }

    pub fn toggle_help(&mut self) {
        self.help_open = !self.help_open;
    }

    pub fn close_help(&mut self) {
        self.help_open = false;
    }

    pub fn modal_scroll_down(&mut self) {
        if self.modal_records.is_empty() {
            return;
        }
        self.modal_selected = (self.modal_selected + 1) % self.modal_records.len();
    }

    pub fn modal_scroll_up(&mut self) {
        if self.modal_records.is_empty() {
            return;
        }
        self.modal_selected = if self.modal_selected == 0 {
            self.modal_records.len() - 1
        } else {
            self.modal_selected - 1
        };
    }

    pub fn modal_h_scroll_left(&mut self) {
        if self.modal_needs_h_scroll && self.modal_h_scroll > 0 {
            self.modal_h_scroll -= 1;
        }
    }

    pub fn modal_h_scroll_right(&mut self) {
        if self.modal_records.is_empty() {
            return;
        }
        let max_cols = self
            .modal_records
            .iter()
            .map(|r| r.headers.len())
            .max()
            .unwrap_or(0);
        if self.modal_needs_h_scroll && self.modal_h_scroll + 1 < max_cols {
            self.modal_h_scroll += 1;
        }
    }

    pub fn modal_select_table(&mut self) {
        if self.modal_records.is_empty() {
            self.close_modal();
            return;
        }
        let target_table = self.modal_records[self.modal_selected].table_name.clone();
        self.close_modal();
        if let Some(idx) = self.tables.iter().position(|t| t == &target_table) {
            self.selected = idx;
            let _ = self.load_table(idx);
        }
    }

    // Filter mode methods

    pub fn toggle_filter_mode(&mut self) {
        if !self.table_focused {
            return;
        }
        match self.filter_mode {
            FilterMode::None => {
                self.filter_mode = FilterMode::HeaderSelect;
                self.filter_col = 0;
            }
            _ => {
                self.cancel_filter_mode();
            }
        }
    }

    pub fn cancel_filter_mode(&mut self) {
        self.filter_mode = FilterMode::None;
        self.temp_filter_op = FilterOp::Equals;
        self.temp_filter_value = String::new();
    }

    pub fn move_filter_col_left(&mut self) {
        if self.filter_mode == FilterMode::HeaderSelect && self.filter_col > 0 {
            self.filter_col -= 1;
        }
    }

    pub fn move_filter_col_right(&mut self) {
        if self.filter_mode == FilterMode::HeaderSelect {
            if self.filter_col + 1 < self.headers.len() {
                self.filter_col += 1;
            }
        }
    }

    pub fn cycle_sort_order(&mut self) {
        if self.filter_mode == FilterMode::HeaderSelect {
            match self.sort_col {
                None => {
                    self.sort_col = Some(self.filter_col);
                    self.sort_asc = true;
                }
                Some(col) if col == self.filter_col => {
                    if self.sort_asc {
                        self.sort_asc = false;
                    } else {
                        self.sort_col = None;
                    }
                }
                Some(_) => {
                    self.sort_col = Some(self.filter_col);
                    self.sort_asc = true;
                }
            }
            let _ = self.apply_filters_and_sort();
        }
    }

    pub fn enter_filter_for_col(&mut self) {
        if self.filter_mode == FilterMode::HeaderSelect {
            // Pre-populate with existing filter if any
            if let Some((op, val)) = &self.filters[self.filter_col] {
                self.temp_filter_op = op.clone();
                self.temp_filter_value = val.clone();
            } else {
                self.temp_filter_op = FilterOp::Equals;
                self.temp_filter_value = String::new();
            }
            self.filter_mode = FilterMode::TypeSelect;
        }
    }

    pub fn toggle_filter_type(&mut self) {
        if self.filter_mode == FilterMode::TypeSelect {
            self.temp_filter_op = match self.temp_filter_op {
                FilterOp::Equals => FilterOp::Contains,
                FilterOp::Contains => FilterOp::Equals,
            };
        }
    }

    pub fn move_to_value_input(&mut self) {
        if self.filter_mode == FilterMode::TypeSelect {
            self.filter_mode = FilterMode::ValueInput;
        }
    }

    pub fn filter_input_char(&mut self, c: char) {
        if self.filter_mode == FilterMode::ValueInput {
            self.temp_filter_value.push(c);
        }
    }

    pub fn filter_input_backspace(&mut self) {
        if self.filter_mode == FilterMode::ValueInput {
            self.temp_filter_value.pop();
        }
    }

    pub fn apply_filter(&mut self) {
        if self.filter_mode == FilterMode::ValueInput {
            if !self.temp_filter_value.is_empty() {
                self.filters[self.filter_col] =
                    Some((self.temp_filter_op.clone(), self.temp_filter_value.clone()));
            }
            self.filter_mode = FilterMode::None;
            self.temp_filter_op = FilterOp::Equals;
            self.temp_filter_value = String::new();
            let _ = self.apply_filters_and_sort();
        }
    }

    pub fn apply_filters_and_sort(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.tables.is_empty() {
            return Ok(());
        }
        let table_name = &self.tables[self.selected];
        let col_count = self.headers.len();

        self.rows = self.fetch_rows(table_name, 0, 100, col_count)?;
        self.has_more_rows = self.rows.len() == 100;
        self.scroll_offset = 0;
        if self.table_focused && !self.rows.is_empty() {
            self.table_state = TableState::new().with_selected(Some(0));
        } else {
            self.table_state = TableState::new();
        }
        Ok(())
    }

    pub fn delete_current_filter(&mut self) {
        if self.filter_col < self.filters.len() {
            self.filters[self.filter_col] = None;
            self.filter_mode = FilterMode::None;
            self.temp_filter_op = FilterOp::Equals;
            self.temp_filter_value = String::new();
            let _ = self.apply_filters_and_sort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_db;

    impl App {
        pub fn clear_filter_for_col(&mut self, col: usize) {
            if col < self.filters.len() {
                self.filters[col] = None;
                let _ = self.apply_filters_and_sort();
            }
        }
    }

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
        assert_eq!(app.rows.len(), 100); // limited to 100 rows initially
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

    #[test]
    fn test_fetch_more_rows() {
        let path = "/tmp/squeal_test_fetch_more.db";
        test_db::TestDb::large(path, 250);
        let mut app = App::new(path).unwrap();
        assert_eq!(app.rows.len(), 100);
        assert!(app.has_more_rows);

        app.fetch_more_rows().unwrap();
        assert_eq!(app.rows.len(), 200);
        assert!(app.has_more_rows);

        app.fetch_more_rows().unwrap();
        assert_eq!(app.rows.len(), 250);
        assert!(!app.has_more_rows);

        // Fetching again when no more rows should be a no-op
        app.fetch_more_rows().unwrap();
        assert_eq!(app.rows.len(), 250);
        assert!(!app.has_more_rows);
    }

    #[test]
    fn test_scroll_table_down_fetches_more() {
        let path = "/tmp/squeal_test_scroll_fetch.db";
        test_db::TestDb::large(path, 250);
        let mut app = App::new(path).unwrap();
        app.focus_table();

        // Scroll to bottom of first batch
        for _ in 0..99 {
            app.scroll_table_down();
        }
        assert_eq!(app.table_state.selected(), Some(99));
        assert_eq!(app.rows.len(), 100);
        assert!(app.has_more_rows);

        // One more scroll should trigger fetching
        app.scroll_table_down();
        assert_eq!(app.table_state.selected(), Some(100));
        assert_eq!(app.rows.len(), 200);
        assert!(app.has_more_rows);

        // Scroll to bottom of second batch
        for _ in 0..99 {
            app.scroll_table_down();
        }
        assert_eq!(app.table_state.selected(), Some(199));
        assert_eq!(app.rows.len(), 200);

        // Scroll to trigger final fetch
        app.scroll_table_down();
        assert_eq!(app.table_state.selected(), Some(200));
        assert_eq!(app.rows.len(), 250);
        assert!(!app.has_more_rows);

        // Keep scrolling to the end
        for _ in 0..49 {
            app.scroll_table_down();
        }
        assert_eq!(app.table_state.selected(), Some(249));
        assert_eq!(app.rows.len(), 250);
        assert!(!app.has_more_rows);

        // Scroll past the end should stay at the bottom
        app.scroll_table_down();
        assert_eq!(app.table_state.selected(), Some(249));
        assert_eq!(app.rows.len(), 250);
    }

    #[test]
    fn test_small_table_no_fetch() {
        let path = "/tmp/squeal_test_small_no_fetch.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(path).unwrap();
        app.focus_table();

        // products has 2 rows, so has_more_rows should be false
        assert!(!app.has_more_rows);

        app.scroll_table_down();
        assert_eq!(app.table_state.selected(), Some(1));
        app.scroll_table_down();
        assert_eq!(app.table_state.selected(), Some(1)); // stays at bottom
        assert_eq!(app.rows.len(), 2);
    }

    #[test]
    fn test_page_down_scrolls_view_and_preserves_visual_position() {
        let path = "/tmp/squeal_test_page_down.db";
        test_db::TestDb::large(path, 250);
        let mut app = App::new(path).unwrap();
        app.focus_table();
        app.page_size = 10;

        // visual position 0 preserved
        app.page_down();
        assert_eq!(app.scroll_offset, 10);
        assert_eq!(app.table_state.selected(), Some(10));

        app.page_down();
        assert_eq!(app.scroll_offset, 20);
        assert_eq!(app.table_state.selected(), Some(20));
    }

    #[test]
    fn test_page_up_preserves_visual_position() {
        let path = "/tmp/squeal_test_page_up.db";
        test_db::TestDb::large(path, 250);
        let mut app = App::new(path).unwrap();
        app.focus_table();
        app.page_size = 10;
        app.scroll_offset = 50;
        app.table_state.select(Some(55));

        app.page_up();
        assert_eq!(app.scroll_offset, 40);
        assert_eq!(app.table_state.selected(), Some(45)); // visual pos 5 preserved
    }

    #[test]
    fn test_page_down_clamps_cursor_on_partial_page() {
        // 15 rows with page_size=10: pages 0-9, 10-14
        let path = "/tmp/squeal_test_page_down_clamp.db";
        test_db::TestDb::large(path, 15);
        let mut app = App::new(path).unwrap();
        app.focus_table();
        app.page_size = 10;
        app.scroll_offset = 0;
        app.table_state.select(Some(9)); // visual pos 9

        app.page_down();
        assert_eq!(app.scroll_offset, 10);
        assert_eq!(app.table_state.selected(), Some(14)); // clamped to last row
    }

    #[test]
    fn test_page_up_at_top_is_noop() {
        let path = "/tmp/squeal_test_page_up_clamp.db";
        test_db::TestDb::large(path, 250);
        let mut app = App::new(path).unwrap();
        app.focus_table();
        app.page_size = 10;
        app.scroll_offset = 0;
        app.table_state.select(Some(5));

        app.page_up();
        assert_eq!(app.scroll_offset, 0);
        assert_eq!(app.table_state.selected(), Some(5)); // no change
    }

    #[test]
    fn test_page_down_to_last_page_preserves_visual_position() {
        let path = "/tmp/squeal_test_small_final.db";
        test_db::TestDb::large(path, 25);
        let mut app = App::new(path).unwrap();
        app.focus_table();
        app.page_size = 10;

        // Page 1: 0-9
        app.page_down();
        assert_eq!(app.scroll_offset, 10);
        assert_eq!(app.table_state.selected(), Some(10));

        // Page 2: 10-19
        app.page_down();
        assert_eq!(app.scroll_offset, 20);
        assert_eq!(app.table_state.selected(), Some(20)); // visual pos 0 preserved
    }

    #[test]
    fn test_page_up_from_last_page_preserves_visual_position() {
        let path = "/tmp/squeal_test_up_from_bottom.db";
        test_db::TestDb::large(path, 25);
        let mut app = App::new(path).unwrap();
        app.focus_table();
        app.page_size = 10;

        // Page 1: 0-9
        app.page_down();
        assert_eq!(app.scroll_offset, 10);
        assert_eq!(app.table_state.selected(), Some(10));

        // Page 2: 10-19
        app.page_down();
        assert_eq!(app.scroll_offset, 20);
        assert_eq!(app.table_state.selected(), Some(20));

        // Page 1: visual pos 0 preserved
        app.page_up();
        assert_eq!(app.scroll_offset, 10);
        assert_eq!(app.table_state.selected(), Some(10));
    }

    #[test]
    fn test_page_down_from_last_page_is_noop() {
        let path = "/tmp/squeal_test_page_down_noop.db";
        test_db::TestDb::large(path, 25);
        let mut app = App::new(path).unwrap();
        app.focus_table();
        app.page_size = 10;

        app.page_down(); // 10
        app.page_down(); // 20 (last page)
        app.page_down(); // should be no-op
        assert_eq!(app.scroll_offset, 20);
        assert_eq!(app.table_state.selected(), Some(20));
    }

    #[test]
    fn test_page_up_from_partial_page_preserves_visual_position() {
        // 15 rows with page_size=10: pages 0-9, 10-14
        let path = "/tmp/squeal_test_up_from_partial.db";
        test_db::TestDb::large(path, 15);
        let mut app = App::new(path).unwrap();
        app.focus_table();
        app.page_size = 10;

        app.page_down(); // offset 10
        app.table_state.select(Some(12)); // visual pos 2
        app.page_up(); // offset 0
        assert_eq!(app.scroll_offset, 0);
        assert_eq!(app.table_state.selected(), Some(2)); // visual pos 2 preserved
    }

    #[test]
    fn test_scroll_table_down_keeps_cursor_visible() {
        let path = "/tmp/squeal_test_cursor_vis.db";
        test_db::TestDb::large(path, 250);
        let mut app = App::new(path).unwrap();
        app.focus_table();
        app.page_size = 10;

        // Move cursor to row 15
        for _ in 0..15 {
            app.scroll_table_down();
        }
        assert_eq!(app.table_state.selected(), Some(15));
        assert_eq!(app.scroll_offset, 6); // window shifted to keep cursor visible
    }

    #[test]
    fn test_open_modal_fetches_fk_records() {
        let conn = test_db::TestDb::in_memory_demo();
        let mut app = App::from_connection(conn).unwrap();
        // Navigate to orders table (should be index 2 after sorting: categories, orders, products, users)
        app.selected = app.tables.iter().position(|t| t == "orders").unwrap();
        app.load_table(app.selected).unwrap();
        app.focus_table();
        assert_eq!(app.table_state.selected(), Some(0));

        app.open_modal().unwrap();
        assert!(app.modal_open);
        assert!(!app.modal_records.is_empty());

        // orders row 0 has user_id = 2 and product_id = 2 (i=1 in the loop)
        let user_record = app
            .modal_records
            .iter()
            .find(|r| r.table_name == "users");
        assert!(user_record.is_some());
        let user_record = user_record.unwrap();
        assert_eq!(user_record.fk_column, "user_id");
        assert_eq!(user_record.fk_value, "2");
        assert_eq!(user_record.headers, vec!["id", "first_name", "last_name", "email", "age", "country", "registered_at"]);
        assert_eq!(user_record.row[0], "2"); // id
        assert_eq!(user_record.row[1], "Charlie"); // first_name

        let product_record = app
            .modal_records
            .iter()
            .find(|r| r.table_name == "products");
        assert!(product_record.is_some());
        let product_record = product_record.unwrap();
        assert_eq!(product_record.fk_column, "product_id");
        assert_eq!(product_record.fk_value, "2");
    }

    #[test]
    fn test_close_modal() {
        let conn = test_db::TestDb::in_memory_demo();
        let mut app = App::from_connection(conn).unwrap();
        app.selected = app.tables.iter().position(|t| t == "orders").unwrap();
        app.load_table(app.selected).unwrap();
        app.focus_table();
        app.open_modal().unwrap();
        assert!(app.modal_open);
        assert!(!app.modal_records.is_empty());

        app.close_modal();
        assert!(!app.modal_open);
        assert!(app.modal_records.is_empty());
    }

    #[test]
    fn test_unfocus_table_closes_modal() {
        let conn = test_db::TestDb::in_memory_demo();
        let mut app = App::from_connection(conn).unwrap();
        app.selected = app.tables.iter().position(|t| t == "orders").unwrap();
        app.load_table(app.selected).unwrap();
        app.focus_table();
        app.open_modal().unwrap();
        assert!(app.modal_open);

        app.unfocus_table();
        assert!(!app.modal_open);
        assert!(app.modal_records.is_empty());
    }

    #[test]
    fn test_load_table_closes_modal() {
        let conn = test_db::TestDb::in_memory_demo();
        let mut app = App::from_connection(conn).unwrap();
        app.selected = app.tables.iter().position(|t| t == "orders").unwrap();
        app.load_table(app.selected).unwrap();
        app.focus_table();
        app.open_modal().unwrap();
        assert!(app.modal_open);

        // Load a different table
        app.selected = app.tables.iter().position(|t| t == "users").unwrap();
        app.load_table(app.selected).unwrap();
        assert!(!app.modal_open);
        assert!(app.modal_records.is_empty());
    }

    #[test]
    fn test_open_modal_no_fks() {
        let conn = test_db::TestDb::in_memory_demo();
        let mut app = App::from_connection(conn).unwrap();
        // users table has no foreign keys
        app.selected = app.tables.iter().position(|t| t == "users").unwrap();
        app.load_table(app.selected).unwrap();
        app.focus_table();
        app.open_modal().unwrap();
        assert!(!app.modal_open);
        assert!(app.modal_records.is_empty());
    }

    // Filter mode tests

    #[test]
    fn test_filter_mode_toggle() {
        let path = "/tmp/squeal_test_filter_toggle.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(path).unwrap();

        // Cannot toggle when not focused
        app.toggle_filter_mode();
        assert_eq!(app.filter_mode, FilterMode::None);

        app.focus_table();
        app.toggle_filter_mode();
        assert_eq!(app.filter_mode, FilterMode::HeaderSelect);
        assert_eq!(app.filter_col, 0);

        app.toggle_filter_mode();
        assert_eq!(app.filter_mode, FilterMode::None);
    }

    #[test]
    fn test_filter_mode_blocked_when_not_focused() {
        let path = "/tmp/squeal_test_filter_block.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(path).unwrap();
        assert!(!app.table_focused);

        app.move_filter_col_right();
        assert_eq!(app.filter_col, 0);
        app.cycle_sort_order();
        assert_eq!(app.sort_col, None);
    }

    #[test]
    fn test_cycle_sort_order() {
        let path = "/tmp/squeal_test_sort.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(path).unwrap();
        app.focus_table();
        app.toggle_filter_mode();

        // No sort -> Asc on col 0
        app.cycle_sort_order();
        assert_eq!(app.sort_col, Some(0));
        assert!(app.sort_asc);

        // Asc -> Desc
        app.cycle_sort_order();
        assert_eq!(app.sort_col, Some(0));
        assert!(!app.sort_asc);

        // Desc -> None
        app.cycle_sort_order();
        assert_eq!(app.sort_col, None);

        // Move to col 1 and sort
        app.move_filter_col_right();
        app.cycle_sort_order();
        assert_eq!(app.sort_col, Some(1));
        assert!(app.sort_asc);

        // Move back to col 0, should set new sort
        app.move_filter_col_left();
        app.cycle_sort_order();
        assert_eq!(app.sort_col, Some(0));
        assert!(app.sort_asc);
    }

    #[test]
    fn test_filter_input() {
        let path = "/tmp/squeal_test_filter_input.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(path).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        app.enter_filter_for_col();

        assert_eq!(app.filter_mode, FilterMode::TypeSelect);
        app.toggle_filter_type();
        assert_eq!(app.temp_filter_op, FilterOp::Contains);
        app.move_to_value_input();
        assert_eq!(app.filter_mode, FilterMode::ValueInput);

        app.filter_input_char('W');
        app.filter_input_char('i');
        assert_eq!(app.temp_filter_value, "Wi");

        app.filter_input_backspace();
        assert_eq!(app.temp_filter_value, "W");
    }

    #[test]
    fn test_filter_navigation() {
        let path = "/tmp/squeal_test_filter_nav.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(path).unwrap();
        app.focus_table();
        app.toggle_filter_mode();

        assert_eq!(app.filter_mode, FilterMode::HeaderSelect);
        app.move_filter_col_right();
        assert_eq!(app.filter_col, 1);
        app.move_filter_col_left();
        assert_eq!(app.filter_col, 0);

        app.enter_filter_for_col();
        assert_eq!(app.filter_mode, FilterMode::TypeSelect);
        app.toggle_filter_type();
        assert_eq!(app.temp_filter_op, FilterOp::Contains);
        app.move_to_value_input();
        assert_eq!(app.filter_mode, FilterMode::ValueInput);

        app.cancel_filter_mode();
        assert_eq!(app.filter_mode, FilterMode::None);
    }

    #[test]
    fn test_filter_applies_and_sorts() {
        let path = "/tmp/squeal_test_filter_apply.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(path).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        app.move_filter_col_right(); // move to title column
        app.enter_filter_for_col();
        app.toggle_filter_type(); // switch to Contains
        app.move_to_value_input();
        app.filter_input_char('W');
        app.apply_filter();

        assert_eq!(app.filter_mode, FilterMode::None);
        assert_eq!(app.rows.len(), 1);
        assert_eq!(app.rows[0], vec!["1", "Widget", "9.99"]);
    }

    #[test]
    fn test_filter_empty_returns_all() {
        let path = "/tmp/squeal_test_filter_empty.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(path).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        app.apply_filters_and_sort().unwrap();

        assert_eq!(app.rows.len(), 2);
    }

    #[test]
    fn test_sort_ascending() {
        let path = "/tmp/squeal_test_sort_asc.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(path).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        app.move_filter_col_right(); // select title column
        app.cycle_sort_order(); // asc
        app.apply_filters_and_sort().unwrap();

        assert_eq!(app.rows.len(), 2);
        assert_eq!(app.rows[0], vec!["2", "Gadget", "19.99"]);
        assert_eq!(app.rows[1], vec!["1", "Widget", "9.99"]);
    }

    #[test]
    fn test_sort_descending() {
        let path = "/tmp/squeal_test_sort_desc.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(path).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        app.move_filter_col_right(); // select title column
        app.cycle_sort_order(); // asc
        app.cycle_sort_order(); // desc
        app.apply_filters_and_sort().unwrap();

        assert_eq!(app.rows.len(), 2);
        assert_eq!(app.rows[0], vec!["1", "Widget", "9.99"]);
        assert_eq!(app.rows[1], vec!["2", "Gadget", "19.99"]);
    }

    #[test]
    fn test_filter_and_sort_combined() {
        let path = "/tmp/squeal_test_filter_sort.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(path).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        app.move_filter_col_right(); // move to title column
        app.enter_filter_for_col();
        app.toggle_filter_type(); // switch to Contains
        app.move_to_value_input();
        app.filter_input_char('e');
        app.apply_filter();

        // Both Widget and Gadget contain 'e'
        assert_eq!(app.rows.len(), 2);

        // Now sort by title descending
        app.toggle_filter_mode();
        app.move_filter_col_right(); // select title column
        app.cycle_sort_order(); // asc
        app.cycle_sort_order(); // desc
        app.apply_filters_and_sort().unwrap();

        assert_eq!(app.rows[0], vec!["1", "Widget", "9.99"]);
        assert_eq!(app.rows[1], vec!["2", "Gadget", "19.99"]);
    }

    #[test]
    fn test_filter_case_insensitive() {
        let path = "/tmp/squeal_test_filter_ci.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(path).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        app.move_filter_col_right(); // move to title column
        app.enter_filter_for_col();
        app.toggle_filter_type(); // switch to Contains
        app.move_to_value_input();
        app.filter_input_char('w');
        app.apply_filter();

        assert_eq!(app.rows.len(), 1);
        assert_eq!(app.rows[0], vec!["1", "Widget", "9.99"]);
    }

    #[test]
    fn test_unfocus_clears_filter_mode() {
        let path = "/tmp/squeal_test_unfocus_filter.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(path).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        assert_eq!(app.filter_mode, FilterMode::HeaderSelect);

        app.unfocus_table();
        assert_eq!(app.filter_mode, FilterMode::None);
        assert!(!app.table_focused);
    }

    #[test]
    fn test_filter_on_multiple_columns() {
        let path = "/tmp/squeal_test_filter_multi.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(path).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        app.enter_filter_for_col(); // filter on id column
        app.move_to_value_input();
        app.filter_input_char('1');
        app.apply_filter();

        // Now add another filter on price using Contains
        app.toggle_filter_mode();
        app.move_filter_col_right();
        app.move_filter_col_right(); // price column
        app.enter_filter_for_col();
        app.toggle_filter_type(); // switch to Contains
        app.move_to_value_input();
        app.filter_input_char('9');
        app.apply_filter();

        assert_eq!(app.rows.len(), 1);
        assert_eq!(app.rows[0], vec!["1", "Widget", "9.99"]);
    }

    #[test]
    fn test_filter_equals() {
        let path = "/tmp/squeal_test_filter_equals.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(path).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        app.move_filter_col_right(); // title column
        app.enter_filter_for_col();
        // Equals is default
        app.move_to_value_input();
        app.filter_input_char('W');
        app.filter_input_char('i');
        app.filter_input_char('d');
        app.filter_input_char('g');
        app.filter_input_char('e');
        app.filter_input_char('t');
        app.apply_filter();

        assert_eq!(app.rows.len(), 1);
        assert_eq!(app.rows[0], vec!["1", "Widget", "9.99"]);
    }

    #[test]
    fn test_clear_filter() {
        let path = "/tmp/squeal_test_clear_filter.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(path).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        app.move_filter_col_right(); // title column
        app.enter_filter_for_col();
        app.toggle_filter_type(); // switch to Contains
        app.move_to_value_input();
        app.filter_input_char('W');
        app.apply_filter();

        assert_eq!(app.rows.len(), 1);

        app.clear_filter_for_col(1);
        assert_eq!(app.rows.len(), 2);
    }

    #[test]
    fn test_delete_current_filter() {
        let path = "/tmp/squeal_test_delete_filter.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(path).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        app.move_filter_col_right(); // title column
        app.enter_filter_for_col();
        app.toggle_filter_type(); // switch to Contains
        app.move_to_value_input();
        app.filter_input_char('W');
        app.apply_filter();

        assert_eq!(app.rows.len(), 1);
        assert_eq!(app.filters[1], Some((FilterOp::Contains, "W".to_string())));

        // Delete from HeaderSelect mode
        app.toggle_filter_mode();
        app.move_filter_col_right(); // move back to title column
        app.delete_current_filter();

        assert_eq!(app.filter_mode, FilterMode::None);
        assert_eq!(app.filters[1], None);
        assert_eq!(app.rows.len(), 2);
    }

    #[test]
    fn test_edit_existing_filter() {
        let path = "/tmp/squeal_test_edit_filter.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(path).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        app.move_filter_col_right(); // title column
        app.enter_filter_for_col();
        app.toggle_filter_type(); // switch to Contains
        app.move_to_value_input();
        app.filter_input_char('W');
        app.apply_filter();

        assert_eq!(app.rows.len(), 1);
        assert_eq!(app.filters[1], Some((FilterOp::Contains, "W".to_string())));

        // Re-enter filter mode on same column - should edit existing filter
        app.toggle_filter_mode();
        app.move_filter_col_right(); // title column
        app.enter_filter_for_col();

        // Should have pre-populated with existing filter
        assert_eq!(app.temp_filter_op, FilterOp::Contains);
        assert_eq!(app.temp_filter_value, "W");
        assert_eq!(app.filter_mode, FilterMode::TypeSelect);

        // Change to Equals and update value
        app.toggle_filter_type(); // switch to Equals
        app.move_to_value_input();
        app.filter_input_backspace(); // remove 'W'
        app.filter_input_char('G');
        app.filter_input_char('a');
        app.filter_input_char('d');
        app.filter_input_char('g');
        app.filter_input_char('e');
        app.filter_input_char('t');
        app.apply_filter();

        assert_eq!(app.filter_mode, FilterMode::None);
        assert_eq!(app.filters[1], Some((FilterOp::Equals, "Gadget".to_string())));
        assert_eq!(app.rows.len(), 1);
        assert_eq!(app.rows[0], vec!["2", "Gadget", "19.99"]);
    }
}
