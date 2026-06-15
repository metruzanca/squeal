//! Application state and business logic.
//!
//! This module holds the [`App`] struct, which manages the SQLite connection, the list of
//! database tables, the currently loaded table data, and all navigation/focus state. It also
//! encapsulates the operations for switching tables, focusing/unfocusing the table view, and
//! scrolling both horizontally and vertically within the data panel.

use ratatui::widgets::TableState;
use tui_syntax::{Highlighter, themes, sql};

use crate::driver::{collect_active_filters, DbDriver, FilterOp};
use crate::ui::helpers::cursor_line_col;

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

#[derive(Debug, Clone)]
pub struct Query {
    pub name: String,
    pub sql: String,
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
    pub queries: Vec<Query>,
    pub selected_sidebar: usize,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub driver: Box<dyn DbDriver>,
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
    pub query_text: String,
    pub query_cursor: usize,
    pub query_edit_mode: bool,
    pub query_scroll: usize,
    pub is_query_view: bool,
    pub rename_mode: bool,
    pub rename_value: String,
    pub highlighter: Highlighter,
    pub save_queries: bool,
    pub all_rows: Vec<Vec<String>>,
    pub working_dir: std::path::PathBuf,
}

impl App {
    pub fn new(mut driver: Box<dyn DbDriver>) -> Result<Self, Box<dyn std::error::Error>> {
        let tables = driver.list_tables()?;

        let working_dir = std::env::current_dir().unwrap_or_default();
        let queries = Self::load_queries(&working_dir);

        let mut highlighter = Highlighter::new(themes::one_dark());
        let _ = highlighter.register_language(sql());

        let mut app = App {
            tables,
            queries,
            selected_sidebar: 0,
            headers: Vec::new(),
            rows: Vec::new(),
            driver,
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
            query_text: String::new(),
            query_cursor: 0,
            query_edit_mode: false,
            query_scroll: 0,
            is_query_view: false,
            rename_mode: false,
            rename_value: String::new(),
            highlighter,
            save_queries: false,
            all_rows: Vec::new(),
            working_dir,
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
        self.selected_sidebar = index;
        self.is_query_view = false;
        self.query_edit_mode = false;
        let table_name = &self.tables[index];

        let headers = self.driver.table_columns(table_name)?;

        // Set headers and reset filter state before fetching
        self.headers = headers;
        self.filters = vec![None; self.headers.len()];
        self.filter_mode = FilterMode::None;
        self.filter_col = 0;
        self.sort_col = None;
        self.sort_asc = true;
        self.temp_filter_op = FilterOp::Equals;
        self.temp_filter_value = String::new();

        let rows = self
            .driver
            .fetch_rows(table_name, &self.headers, &self.filters, None, true, 0, 100)?;

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
        if self.tables.is_empty() || !self.has_more_rows || self.is_query_view {
            return Ok(());
        }
        let table_name = &self.tables[self.selected_sidebar];
        let offset = self.rows.len();

        let new_rows = self.driver.fetch_rows(
            table_name,
            &self.headers,
            &self.filters,
            self.sort_col,
            self.sort_asc,
            offset,
            100,
        )?;
        self.has_more_rows = new_rows.len() == 100;
        self.rows.extend(new_rows);
        Ok(())
    }

    pub fn next(&mut self) {
        if self.table_focused {
            return;
        }
        let len = self.sidebar_len();
        if len == 0 {
            return;
        }
        let mut next = (self.selected_sidebar + 1) % len;
        // Skip separator
        if !self.tables.is_empty() && !self.queries.is_empty() && next == self.tables.len() {
            next = (next + 1) % len;
        }
        self.selected_sidebar = next;
        if self.current_is_table() {
            let _ = self.load_table(self.selected_sidebar);
        } else if self.current_is_query() {
            let _ = self.load_query(self.query_index());
        }
    }

    pub fn previous(&mut self) {
        if self.table_focused {
            return;
        }
        let len = self.sidebar_len();
        if len == 0 {
            return;
        }
        let mut prev = if self.selected_sidebar == 0 {
            len - 1
        } else {
            self.selected_sidebar - 1
        };
        // Skip separator
        if !self.tables.is_empty() && !self.queries.is_empty() && prev == self.tables.len() {
            prev = if prev == 0 { len - 1 } else { prev - 1 };
        }
        self.selected_sidebar = prev;
        if self.current_is_table() {
            let _ = self.load_table(self.selected_sidebar);
        } else if self.current_is_query() {
            let _ = self.load_query(self.query_index());
        }
    }

    pub fn focus_table(&mut self) {
        if self.current_is_table() {
            if !self.headers.is_empty() {
                self.table_focused = true;
                self.scroll_offset = 0;
                if !self.rows.is_empty() {
                    self.table_state.select(Some(0));
                }
            }
        } else if self.current_is_query() {
            if !self.is_query_view {
                let _ = self.load_query(self.query_index());
            }
            self.table_focused = true;
            self.query_edit_mode = false;
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
        self.query_edit_mode = false;
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
            if !self.is_query_view && selected + 1 >= self.rows.len() && self.has_more_rows {
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
        while !self.is_query_view && self.rows.len() <= target + self.page_size && self.has_more_rows {
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

    pub fn open_modal(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(selected) = self.table_state.selected() else {
            return Ok(());
        };
        if selected >= self.rows.len() {
            return Ok(());
        }
        if self.is_query_view {
            // Show row data detail for query views
            let row = self.rows[selected].clone();
            let headers = self.headers.clone();
            self.modal_records = vec![RelatedRecord {
                table_name: "Query Result".to_string(),
                fk_column: "Row".to_string(),
                ref_column: "Data".to_string(),
                fk_value: (selected + 1).to_string(),
                headers,
                row,
            }];
            self.modal_open = true;
            self.modal_selected = 0;
            self.modal_h_scroll = 0;
            self.modal_needs_h_scroll = false;
            return Ok(());
        }
        let table_name = &self.tables[self.selected_sidebar];
        let fks = self.driver.get_foreign_keys(table_name)?;
        let row = &self.rows[selected];
        if fks.is_empty() {
            // No FKs: show the current row data as a detail view
            self.modal_records = vec![RelatedRecord {
                table_name: table_name.clone(),
                fk_column: "Row".to_string(),
                ref_column: "Data".to_string(),
                fk_value: (selected + 1).to_string(),
                headers: self.headers.clone(),
                row: row.clone(),
            }];
            self.modal_open = true;
            self.modal_selected = 0;
            self.modal_h_scroll = 0;
            self.modal_needs_h_scroll = false;
            return Ok(());
        }
        let mut records = Vec::new();
        for fk in fks {
            let col_idx = self.headers.iter().position(|h| h == &fk.from);
            let Some(idx) = col_idx else { continue };
            let fk_value = &row[idx];
            if fk_value.is_empty() {
                continue;
            }
            if let Some((ref_headers, ref_row)) =
                self.driver.fetch_related_record(&fk.table, &fk.to, fk_value)?
            {
                records.push(RelatedRecord {
                    table_name: fk.table.clone(),
                    fk_column: fk.from.clone(),
                    ref_column: fk.to.clone(),
                    fk_value: fk_value.clone(),
                    headers: ref_headers,
                    row: ref_row,
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
        if self.is_query_view {
            return self.apply_query_filters_and_sort();
        }
        if self.tables.is_empty() {
            return Ok(());
        }
        let table_name = &self.tables[self.selected_sidebar];

        self.rows = self.driver.fetch_rows(
            table_name,
            &self.headers,
            &self.filters,
            self.sort_col,
            self.sort_asc,
            0,
            100,
        )?;
        self.has_more_rows = self.rows.len() == 100;
        self.scroll_offset = 0;
        if self.table_focused && !self.rows.is_empty() {
            self.table_state = TableState::new().with_selected(Some(0));
        } else {
            self.table_state = TableState::new();
        }
        Ok(())
    }

    fn apply_query_filters_and_sort(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.all_rows.is_empty() {
            self.rows = Vec::new();
            self.scroll_offset = 0;
            self.table_state = TableState::new();
            return Ok(());
        }

        let mut rows = self.all_rows.clone();

        let active_filters = collect_active_filters(&self.filters);

        if !active_filters.is_empty() {
            rows.retain(|row| {
                active_filters.iter().all(|(i, op, val)| {
                    let cell = &row[*i];
                    match op {
                        FilterOp::Equals => cell == *val,
                        FilterOp::Contains => cell.to_lowercase().contains(&val.to_lowercase()),
                    }
                })
            });
        }

        if let Some(sort_col) = self.sort_col {
            rows.sort_by(|a, b| {
                let cmp = a[sort_col].cmp(&b[sort_col]);
                if self.sort_asc {
                    cmp
                } else {
                    cmp.reverse()
                }
            });
        }

        self.rows = rows;
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

    // Query methods

    fn load_queries(working_dir: &std::path::Path) -> Vec<Query> {
        let queries_dir = working_dir.join(".squeal").join("queries");
        let mut queries = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&queries_dir) {
            let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
            entries.sort_by_key(|e| e.path());

            for entry in entries {
                let path = entry.path();
                if path.is_file() {
                    if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                        if let Ok(sql) = std::fs::read_to_string(&path) {
                            queries.push(Query {
                                name: name.to_string(),
                                sql,
                            });
                        }
                    }
                }
            }
        }

        queries
    }

    fn save_query_to_disk(working_dir: &std::path::Path, name: &str, sql: &str) -> Result<(), Box<dyn std::error::Error>> {
        let queries_dir = working_dir.join(".squeal").join("queries");
        std::fs::create_dir_all(&queries_dir)?;
        let path = queries_dir.join(format!("{}.sql", name));
        std::fs::write(&path, sql)?;
        Ok(())
    }

    fn delete_query_from_disk(working_dir: &std::path::Path, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let path = working_dir.join(".squeal").join("queries").join(format!("{}.sql", name));
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    pub fn run_query(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let sql = self.query_text.trim();
        if sql.is_empty() {
            self.headers = Vec::new();
            self.rows = Vec::new();
            return Ok(());
        }

        let (headers, rows) = self.driver.run_query(sql)?;

        self.headers = headers;
        self.rows = rows;

        self.has_more_rows = false;
        self.h_scroll = 0;
        self.scroll_offset = 0;
        self.table_state = TableState::new();
        if self.table_focused && !self.rows.is_empty() {
            self.table_state = TableState::new().with_selected(Some(0));
        }
        // Clear filters/sort when query is re-run
        self.all_rows = self.rows.clone();
        self.filters = vec![None; self.headers.len()];
        self.filter_mode = FilterMode::None;
        self.filter_col = 0;
        self.sort_col = None;
        self.sort_asc = true;
        self.temp_filter_op = FilterOp::Equals;
        self.temp_filter_value = String::new();

        Ok(())
    }

    pub fn load_query(&mut self, index: usize) -> Result<(), Box<dyn std::error::Error>> {
        if index >= self.queries.len() {
            return Ok(());
        }
        let offset = if self.tables.is_empty() { 0 } else { self.tables.len() + 1 };
        self.selected_sidebar = offset + index;
        self.query_text = self.queries[index].sql.clone();
        self.query_cursor = self.query_text.chars().count();
        self.query_scroll = 0;
        self.is_query_view = true;
        self.query_edit_mode = false;
        self.run_query()?;
        Ok(())
    }

    // Sidebar helpers
    pub fn sidebar_len(&self) -> usize {
        let mut len = self.tables.len();
        if !self.queries.is_empty() {
            if !self.tables.is_empty() {
                len += 1; // separator
            }
            len += self.queries.len();
        }
        len
    }

    pub fn current_is_table(&self) -> bool {
        self.selected_sidebar < self.tables.len()
    }

    pub fn current_is_query(&self) -> bool {
        if self.queries.is_empty() {
            false
        } else {
            let offset = if self.tables.is_empty() { 0 } else { self.tables.len() + 1 };
            self.selected_sidebar >= offset
        }
    }

    pub fn query_index(&self) -> usize {
        let offset = if self.tables.is_empty() { 0 } else { self.tables.len() + 1 };
        self.selected_sidebar - offset
    }

    pub fn save_current_query(&mut self) {
        if self.is_query_view && self.current_is_query() {
            let idx = self.query_index();
            let name = self.queries[idx].name.clone();
            if self.save_queries {
                if let Err(e) = Self::save_query_to_disk(&self.working_dir, &name, &self.query_text) {
                    eprintln!("Failed to save query: {}", e);
                } else {
                    self.queries[idx].sql = self.query_text.clone();
                }
            } else {
                self.queries[idx].sql = self.query_text.clone();
            }
        }
    }

    pub fn start_rename(&mut self) {
        if self.current_is_query() {
            let idx = self.query_index();
            self.rename_value = self.queries[idx].name.clone();
            self.rename_mode = true;
        }
    }

    pub fn cancel_rename(&mut self) {
        self.rename_mode = false;
        self.rename_value = String::new();
    }

    pub fn apply_rename(&mut self) {
        if !self.rename_value.is_empty() && self.current_is_query() {
            let idx = self.query_index();
            let old_name = self.queries[idx].name.clone();
            let new_name = self.rename_value.clone();
            if old_name != new_name && !self.queries.iter().any(|q| q.name == new_name) {
                if self.save_queries {
                    let old_path = self.working_dir.join(".squeal").join("queries").join(format!("{}.sql", old_name));
                    let new_path = self.working_dir.join(".squeal").join("queries").join(format!("{}.sql", new_name));
                    if let Err(e) = std::fs::rename(&old_path, &new_path) {
                        eprintln!("Failed to rename query file: {}", e);
                    } else {
                        self.queries[idx].name = new_name;
                    }
                } else {
                    self.queries[idx].name = new_name;
                }
            }
        }
        self.rename_mode = false;
        self.rename_value = String::new();
    }

    pub fn create_new_query(&mut self) {
        let base_name = "new_query";
        let mut name = base_name.to_string();
        let mut i = 1;
        while self.queries.iter().any(|q| q.name == name) {
            name = format!("{}_{}", base_name, i);
            i += 1;
        }

        let query = Query {
            name: name.clone(),
            sql: String::new(),
        };

        if self.save_queries {
            if let Err(e) = Self::save_query_to_disk(&self.working_dir, &name, "") {
                eprintln!("Failed to save query: {}", e);
                return;
            }
        }

        self.queries.push(query);
        let offset = if self.tables.is_empty() { 0 } else { self.tables.len() + 1 };
        self.selected_sidebar = offset + self.queries.len() - 1;
        self.query_text = String::new();
        self.query_cursor = 0;
        self.query_scroll = 0;
        self.is_query_view = true;
        self.query_edit_mode = true;
        self.headers = Vec::new();
        self.rows = Vec::new();
        self.table_focused = true;
        self.table_state = TableState::new();
        self.h_scroll = 0;
        self.scroll_offset = 0;
        self.filters = Vec::new();
        self.sort_col = None;
        self.sort_asc = true;
        self.filter_mode = FilterMode::None;
    }

    pub fn delete_current_query(&mut self) {
        if !self.current_is_query() {
            return;
        }
        let idx = self.query_index();
        let name = self.queries[idx].name.clone();
        if self.save_queries {
            if let Err(e) = Self::delete_query_from_disk(&self.working_dir, &name) {
                eprintln!("Failed to delete query: {}", e);
            }
        }
        self.queries.remove(idx);
        if self.queries.is_empty() {
            self.is_query_view = false;
            self.query_edit_mode = false;
            self.table_focused = false;
            if !self.tables.is_empty() {
                self.selected_sidebar = self.tables.len().saturating_sub(1);
                let _ = self.load_table(self.selected_sidebar);
            } else {
                self.selected_sidebar = 0;
            }
        } else {
            let offset = if self.tables.is_empty() { 0 } else { self.tables.len() + 1 };
            let max_query = self.queries.len() - 1;
            let new_idx = idx.min(max_query);
            self.selected_sidebar = offset + new_idx;
            let _ = self.load_query(new_idx);
        }
    }

    pub fn insert_query_char(&mut self, c: char) {
        let mut chars: Vec<char> = self.query_text.chars().collect();
        if self.query_cursor <= chars.len() {
            chars.insert(self.query_cursor, c);
            self.query_cursor += 1;
            self.query_text = chars.into_iter().collect();
        }
    }

    pub fn backspace_query_char(&mut self) {
        if self.query_cursor > 0 {
            let mut chars: Vec<char> = self.query_text.chars().collect();
            chars.remove(self.query_cursor - 1);
            self.query_cursor -= 1;
            self.query_text = chars.into_iter().collect();
        }
    }

    pub fn delete_query_char(&mut self) {
        let mut chars: Vec<char> = self.query_text.chars().collect();
        if self.query_cursor < chars.len() {
            chars.remove(self.query_cursor);
            self.query_text = chars.into_iter().collect();
        }
    }

    pub fn move_query_cursor_left(&mut self) {
        if self.query_cursor > 0 {
            self.query_cursor -= 1;
        }
    }

    pub fn move_query_cursor_right(&mut self) {
        let len = self.query_text.chars().count();
        if self.query_cursor < len {
            self.query_cursor += 1;
        }
    }

    pub fn move_query_cursor_up(&mut self) {
        let chars: Vec<char> = self.query_text.chars().collect();
        let mut line_starts = vec![0];
        for (i, ch) in chars.iter().enumerate() {
            if *ch == '\n' {
                line_starts.push(i + 1);
            }
        }

        let mut current_line = 0;
        for (i, &start) in line_starts.iter().enumerate() {
            if start > self.query_cursor {
                break;
            }
            current_line = i;
        }

        if current_line == 0 {
            return;
        }

        let current_line_start = line_starts[current_line];
        let prev_line_start = line_starts[current_line - 1];
        let prev_line_end = current_line_start.saturating_sub(1);
        let prev_line_len = prev_line_end - prev_line_start;
        let col = self.query_cursor - current_line_start;

        self.query_cursor = prev_line_start + col.min(prev_line_len);
    }

    pub fn move_query_cursor_down(&mut self) {
        let chars: Vec<char> = self.query_text.chars().collect();
        let mut line_starts = vec![0];
        for (i, ch) in chars.iter().enumerate() {
            if *ch == '\n' {
                line_starts.push(i + 1);
            }
        }

        let mut current_line = 0;
        for (i, &start) in line_starts.iter().enumerate() {
            if start > self.query_cursor {
                break;
            }
            current_line = i;
        }

        if current_line + 1 >= line_starts.len() {
            return;
        }

        let current_line_start = line_starts[current_line];
        let next_line_start = line_starts[current_line + 1];
        let mut next_line_end = chars.len();
        if current_line + 2 < line_starts.len() {
            next_line_end = line_starts[current_line + 2] - 1;
        }
        let next_line_len = next_line_end - next_line_start;
        let col = self.query_cursor - current_line_start;

        self.query_cursor = next_line_start + col.min(next_line_len);
    }

    pub fn move_query_cursor_home(&mut self) {
        let chars: Vec<char> = self.query_text.chars().collect();
        let mut current_line_start = 0;
        for (i, ch) in chars.iter().enumerate() {
            if i == self.query_cursor {
                break;
            }
            if *ch == '\n' {
                current_line_start = i + 1;
            }
        }
        self.query_cursor = current_line_start;
    }

    pub fn move_query_cursor_end(&mut self) {
        let chars: Vec<char> = self.query_text.chars().collect();
        for (i, ch) in chars.iter().enumerate().skip(self.query_cursor) {
            if *ch == '\n' {
                self.query_cursor = i;
                return;
            }
        }
        self.query_cursor = chars.len();
    }

    pub fn ensure_query_cursor_visible(&mut self, area_height: u16) {
        let (line, _) = cursor_line_col(&self.query_text, self.query_cursor);
        let visible_height = area_height as usize;
        if visible_height == 0 {
            return;
        }
        if line >= self.query_scroll + visible_height {
            self.query_scroll = line.saturating_sub(visible_height - 1);
        } else if line < self.query_scroll {
            self.query_scroll = line;
        }
    }
}


