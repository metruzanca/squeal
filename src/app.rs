//! Application state and business logic.
//!
//! This module holds the [`App`] struct, which manages the database connection, the list of
//! database tables, and all navigation/focus state. It also
//! encapsulates the operations for switching tables, focusing/unfocusing the table view, and
//! scrolling both horizontally and vertically within the data panel.

use fuzzy_matcher::FuzzyMatcher;
use ratatui::widgets::TableState;
use tui_syntax::{Highlighter, themes, sql};

use crate::driver::{collect_active_filters, ColumnType, DbDriver, FilterOp, TableInfo};
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

#[derive(Debug, Clone)]
pub struct SchemaGroup {
    pub name: String,
    pub expanded: bool,
    pub table_indices: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SidebarEntry {
    GroupHeader(usize),
    Table(usize),
    Separator,
    Query(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub enum FilterMode {
    None,
    HeaderSelect,
    TypeSelect,
    ValueInput,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FuzzyKind {
    Table,
    Query,
}

pub struct FuzzyEntry {
    pub kind: FuzzyKind,
    pub label: String,
    pub display: String,
    pub table_index: Option<usize>,
    pub query_index: Option<usize>,
}

pub struct App {
    pub tables: Vec<TableInfo>,
    pub queries: Vec<Query>,
    pub groups: Vec<SchemaGroup>,
    pub sidebar_entries: Vec<SidebarEntry>,
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
    pub column_types: Vec<ColumnType>,
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
    pub fuzzy_open: bool,
    pub fuzzy_query: String,
    pub fuzzy_selected: usize,
    pub fuzzy_matches: Vec<usize>,
    pub fuzzy_entries: Vec<FuzzyEntry>,
}

impl App {
    fn build_groups(tables: &[TableInfo]) -> Vec<SchemaGroup> {
        let mut group_map: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();
        for (i, t) in tables.iter().enumerate() {
            let schema = if t.schema.is_empty() { "" } else { &t.schema };
            group_map
                .entry(schema.to_string())
                .or_default()
                .push(i);
        }
        let mut groups: Vec<SchemaGroup> = group_map
            .into_iter()
            .map(|(name, table_indices)| SchemaGroup {
                expanded: name == "public",
                name,
                table_indices,
            })
            .collect();
        // Sort so "public" is always first
        if let Some(pub_idx) = groups.iter().position(|g| g.name == "public") {
            if pub_idx != 0 {
                let g = groups.remove(pub_idx);
                groups.insert(0, g);
            }
        }
        groups
    }

    fn rebuild_sidebar(&mut self) {
        let mut entries = Vec::new();
        let multi_group = self.groups.len() > 1
            || (self.groups.len() == 1 && !self.groups[0].name.is_empty());

        if multi_group {
            for (gi, group) in self.groups.iter().enumerate() {
                entries.push(SidebarEntry::GroupHeader(gi));
                if group.expanded {
                    for &ti in &group.table_indices {
                        entries.push(SidebarEntry::Table(ti));
                    }
                }
            }
        } else {
            // Single group: show tables flat (no group header)
            if let Some(group) = self.groups.first() {
                for &ti in &group.table_indices {
                    entries.push(SidebarEntry::Table(ti));
                }
            }
        }

        if !self.tables.is_empty() && !self.queries.is_empty() {
            entries.push(SidebarEntry::Separator);
        }
        for (qi, _) in self.queries.iter().enumerate() {
            entries.push(SidebarEntry::Query(qi));
        }
        // Clamp selected_sidebar
        if !entries.is_empty() && self.selected_sidebar >= entries.len() {
            self.selected_sidebar = entries.len().saturating_sub(1);
        }
        self.sidebar_entries = entries;
    }

    fn table_ident(&self, index: usize) -> String {
        let t = &self.tables[index];
        if t.schema.is_empty() {
            t.name.clone()
        } else {
            format!("{}.{}", t.schema, t.name)
        }
    }

    fn table_ident_or_empty(&self, index: usize) -> String {
        if index >= self.tables.len() {
            String::new()
        } else {
            self.table_ident(index)
        }
    }

    pub fn toggle_group(&mut self, group_idx: usize) {
        if group_idx < self.groups.len() {
            let g = &mut self.groups[group_idx];
            g.expanded = !g.expanded;
            self.rebuild_sidebar();
        }
    }

    pub fn is_on_group_header(&self) -> bool {
        if let Some(entry) = self.sidebar_entries.get(self.selected_sidebar) {
            matches!(entry, SidebarEntry::GroupHeader(_))
        } else {
            false
        }
    }

    pub fn current_group_index(&self) -> Option<usize> {
        match self.sidebar_entries.get(self.selected_sidebar) {
            Some(SidebarEntry::GroupHeader(gi)) => Some(*gi),
            _ => None,
        }
    }

    pub fn new(mut driver: Box<dyn DbDriver>) -> Result<Self, Box<dyn std::error::Error>> {
        let tables = driver.list_tables()?;

        let working_dir = std::env::current_dir().unwrap_or_default();
        let queries = Self::load_queries(&working_dir);

        let mut highlighter = Highlighter::new(themes::one_dark());
        let _ = highlighter.register_language(sql());

        let groups = Self::build_groups(&tables);

        let mut app = App {
            groups,
            sidebar_entries: Vec::new(),
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
            column_types: Vec::new(),
            working_dir,
            fuzzy_open: false,
            fuzzy_query: String::new(),
            fuzzy_selected: 0,
            fuzzy_matches: Vec::new(),
            fuzzy_entries: Vec::new(),
        };

        app.rebuild_sidebar();

        if !app.tables.is_empty() {
            app.load_table(0)?;
        }

        Ok(app)
    }

    pub fn load_table(&mut self, index: usize) -> Result<(), Box<dyn std::error::Error>> {
        if index >= self.tables.len() {
            return Ok(());
        }
        self.is_query_view = false;
        self.query_edit_mode = false;
        // Update selected_sidebar to point to this table's sidebar entry
        for (i, entry) in self.sidebar_entries.iter().enumerate() {
            if let SidebarEntry::Table(ti) = entry {
                if *ti == index {
                    self.selected_sidebar = i;
                    break;
                }
            }
        }
        let table_ident = self.table_ident(index);

        let headers = self.driver.table_columns(&table_ident)?;
        let column_types = self.driver.table_column_types(&table_ident)?;

        // Set headers and reset filter state before fetching
        self.headers = headers;
        self.column_types = column_types;
        self.filters = vec![None; self.headers.len()];
        self.filter_mode = FilterMode::None;
        self.filter_col = 0;
        self.sort_col = None;
        self.sort_asc = true;
        self.temp_filter_op = FilterOp::Equals;
        self.temp_filter_value = String::new();

        let rows = self
            .driver
            .fetch_rows(&table_ident, &self.headers, &self.filters, None, true, 0, 100)?;

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
        let table_index = self.current_table_index();
        let Some(ti) = table_index else {
            return Ok(());
        };
        let table_ident = self.table_ident(ti);
        let offset = self.rows.len();

        let new_rows = self.driver.fetch_rows(
            &table_ident,
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
        let len = self.sidebar_entries.len();
        if len == 0 {
            return;
        }
        let mut next = (self.selected_sidebar + 1) % len;
        // Skip separator
        if let Some(SidebarEntry::Separator) = self.sidebar_entries.get(next) {
            next = (next + 1) % len;
        }
        self.selected_sidebar = next;
        match self.sidebar_entries.get(next) {
            Some(SidebarEntry::Table(ti)) => {
                let _ = self.load_table(*ti);
            }
            Some(SidebarEntry::Query(qi)) => {
                let _ = self.load_query(*qi);
            }
            _ => {}
        }
    }

    pub fn previous(&mut self) {
        if self.table_focused {
            return;
        }
        let len = self.sidebar_entries.len();
        if len == 0 {
            return;
        }
        let mut prev = if self.selected_sidebar == 0 {
            len - 1
        } else {
            self.selected_sidebar - 1
        };
        // Skip separator
        while let Some(SidebarEntry::Separator) = self.sidebar_entries.get(prev) {
            if prev == 0 {
                prev = len - 1;
            } else {
                prev -= 1;
            }
        }
        self.selected_sidebar = prev;
        match self.sidebar_entries.get(prev) {
            Some(SidebarEntry::Table(ti)) => {
                let _ = self.load_table(*ti);
            }
            Some(SidebarEntry::Query(qi)) => {
                let _ = self.load_query(*qi);
            }
            _ => {}
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
        let table_index = self.current_table_index().unwrap_or(self.selected_sidebar);
        let table_ident = self.table_ident_or_empty(table_index);
        let fks = self.driver.get_foreign_keys(&table_ident)?;
        let row = &self.rows[selected];
        if fks.is_empty() {
            // No FKs: show the current row data as a detail view
            self.modal_records = vec![RelatedRecord {
                table_name: table_ident.clone(),
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
        if let Some(idx) = self.tables.iter().position(|t| {
            let ti = if t.schema.is_empty() {
                t.name.clone()
            } else {
                format!("{}.{}", t.schema, t.name)
            };
            ti == target_table
        }) {
            let _ = self.load_table(idx);
        } else if let Some(idx) = self.tables.iter().position(|t| t.name == target_table) {
            let _ = self.load_table(idx);
        }
    }

    // Filter mode methods

    fn default_filter_op(&self, col: usize) -> FilterOp {
        if col < self.column_types.len() && self.column_types[col] == ColumnType::String {
            FilterOp::Contains
        } else {
            FilterOp::Equals
        }
    }

    fn filter_ops_for_col(&self, col: usize) -> Vec<FilterOp> {
        let col_type = self.column_types.get(col).cloned().unwrap_or(ColumnType::Other);
        match col_type {
            ColumnType::Number => vec![
                FilterOp::Equals,
                FilterOp::NotEquals,
                FilterOp::Contains,
                FilterOp::GreaterThan,
                FilterOp::LessThan,
                FilterOp::GreaterThanOrEquals,
                FilterOp::LessThanOrEquals,
            ],
            _ => vec![
                FilterOp::Contains,
                FilterOp::Equals,
                FilterOp::NotEquals,
            ],
        }
    }

    fn next_filter_op(&self, current: &FilterOp, col: usize) -> FilterOp {
        let ops = self.filter_ops_for_col(col);
        let pos = ops.iter().position(|op| op == current).unwrap_or(0);
        ops[(pos + 1) % ops.len()].clone()
    }

    fn prev_filter_op(&self, current: &FilterOp, col: usize) -> FilterOp {
        let ops = self.filter_ops_for_col(col);
        let pos = ops.iter().position(|op| op == current).unwrap_or(0);
        let prev = if pos == 0 { ops.len() - 1 } else { pos - 1 };
        ops[prev].clone()
    }

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
                self.temp_filter_op = self.default_filter_op(self.filter_col);
                self.temp_filter_value = String::new();
            }
            self.filter_mode = FilterMode::TypeSelect;
        }
    }

    pub fn toggle_filter_type(&mut self) {
        if self.filter_mode == FilterMode::TypeSelect {
            self.temp_filter_op = self.next_filter_op(&self.temp_filter_op, self.filter_col);
        }
    }

    pub fn toggle_filter_type_back(&mut self) {
        if self.filter_mode == FilterMode::TypeSelect {
            self.temp_filter_op = self.prev_filter_op(&self.temp_filter_op, self.filter_col);
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

    pub fn refresh_current_table(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.tables.is_empty() || self.is_query_view || self.headers.is_empty() {
            return Ok(());
        }
        let ti = self.current_table_index().unwrap_or(self.selected_sidebar);
        let table_ident = self.table_ident_or_empty(ti);
        let fetch_count = self.rows.len().max(100);
        let new_rows = self.driver.fetch_rows(
            &table_ident,
            &self.headers,
            &self.filters,
            self.sort_col,
            self.sort_asc,
            0,
            fetch_count,
        )?;
        self.has_more_rows = new_rows.len() == fetch_count;
        self.rows = new_rows;
        if let Some(selected) = self.table_state.selected() {
            if selected >= self.rows.len() && !self.rows.is_empty() {
                self.table_state.select(Some(self.rows.len() - 1));
            }
        }
        Ok(())
    }

    pub fn apply_filters_and_sort(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.is_query_view {
            return self.apply_query_filters_and_sort();
        }
        if self.tables.is_empty() {
            return Ok(());
        }
        let ti = self.current_table_index().unwrap_or(self.selected_sidebar);
        let table_ident = self.table_ident_or_empty(ti);

        self.rows = self.driver.fetch_rows(
            &table_ident,
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
                        FilterOp::NotEquals => cell != *val,
                        FilterOp::Contains => cell.to_lowercase().contains(&val.to_lowercase()),
                        FilterOp::GreaterThan => {
                            match (cell.parse::<f64>(), val.parse::<f64>()) {
                                (Ok(c), Ok(v)) => c > v,
                                _ => cell > *val,
                            }
                        }
                        FilterOp::LessThan => {
                            match (cell.parse::<f64>(), val.parse::<f64>()) {
                                (Ok(c), Ok(v)) => c < v,
                                _ => cell < *val,
                            }
                        }
                        FilterOp::GreaterThanOrEquals => {
                            match (cell.parse::<f64>(), val.parse::<f64>()) {
                                (Ok(c), Ok(v)) => c >= v,
                                _ => cell >= *val,
                            }
                        }
                        FilterOp::LessThanOrEquals => {
                            match (cell.parse::<f64>(), val.parse::<f64>()) {
                                (Ok(c), Ok(v)) => c <= v,
                                _ => cell <= *val,
                            }
                        }
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
        self.column_types = vec![ColumnType::String; self.headers.len()];
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
        self.query_text = self.queries[index].sql.clone();
        self.query_cursor = self.query_text.chars().count();
        self.query_scroll = 0;
        self.is_query_view = true;
        self.query_edit_mode = false;
        self.run_query()?;
        // Find the sidebar entry for this query and set selected_sidebar
        for (i, entry) in self.sidebar_entries.iter().enumerate() {
            if let SidebarEntry::Query(qi) = entry {
                if *qi == index {
                    self.selected_sidebar = i;
                    break;
                }
            }
        }
        Ok(())
    }

    // Sidebar helpers
    pub fn sidebar_len(&self) -> usize {
        self.sidebar_entries.len()
    }

    pub fn current_is_table(&self) -> bool {
        match self.sidebar_entries.get(self.selected_sidebar) {
            Some(SidebarEntry::Table(_)) => true,
            _ => false,
        }
    }

    pub fn current_is_query(&self) -> bool {
        match self.sidebar_entries.get(self.selected_sidebar) {
            Some(SidebarEntry::Query(_)) => true,
            _ => false,
        }
    }

    pub fn current_table_index(&self) -> Option<usize> {
        match self.sidebar_entries.get(self.selected_sidebar) {
            Some(SidebarEntry::Table(ti)) => Some(*ti),
            _ => None,
        }
    }

    pub fn query_index(&self) -> usize {
        match self.sidebar_entries.get(self.selected_sidebar) {
            Some(SidebarEntry::Query(qi)) => *qi,
            _ => 0,
        }
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
        self.rebuild_sidebar();
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
        self.rebuild_sidebar();
        if self.queries.is_empty() {
            self.is_query_view = false;
            self.query_edit_mode = false;
            self.table_focused = false;
            if !self.tables.is_empty() {
                if let Some(first_table) = self.sidebar_entries.iter().position(|e| matches!(e, SidebarEntry::Table(_))) {
                    self.selected_sidebar = first_table;
                    if let Some(SidebarEntry::Table(ti)) = self.sidebar_entries.get(first_table) {
                        let _ = self.load_table(*ti);
                    }
                }
            } else {
                self.selected_sidebar = 0;
            }
        } else {
            let new_idx = idx.min(self.queries.len() - 1);
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

    // Fuzzy finder methods

    pub fn toggle_fuzzy(&mut self) {
        if self.fuzzy_open {
            self.close_fuzzy();
            return;
        }
        self.fuzzy_open = true;
        self.fuzzy_query = String::new();
        self.fuzzy_selected = 0;
        self.build_fuzzy_entries();
    }

    pub fn close_fuzzy(&mut self) {
        self.fuzzy_open = false;
        self.fuzzy_query = String::new();
        self.fuzzy_selected = 0;
        self.fuzzy_matches = Vec::new();
        self.fuzzy_entries = Vec::new();
    }

    fn build_fuzzy_entries(&mut self) {
        let mut entries = Vec::new();
        for (ti, table) in self.tables.iter().enumerate() {
            let label = if table.schema.is_empty() {
                table.name.clone()
            } else {
                format!("{}.{}", table.schema, table.name)
            };
            let display = format!("[T] {}", label);
            entries.push(FuzzyEntry {
                kind: FuzzyKind::Table,
                label,
                display,
                table_index: Some(ti),
                query_index: None,
            });
        }
        for (qi, query) in self.queries.iter().enumerate() {
            let display = format!("[Q] {}", query.name);
            entries.push(FuzzyEntry {
                kind: FuzzyKind::Query,
                label: query.name.clone(),
                display,
                table_index: None,
                query_index: Some(qi),
            });
        }
        self.fuzzy_entries = entries;
        self.apply_fuzzy_filter();
    }

    fn apply_fuzzy_filter(&mut self) {
        let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
        let query = self.fuzzy_query.trim();
        if query.is_empty() {
            self.fuzzy_matches = (0..self.fuzzy_entries.len()).collect();
            self.fuzzy_selected = 0;
            return;
        }
        let mut scored: Vec<(i64, usize)> = self
            .fuzzy_entries
            .iter()
            .enumerate()
            .filter_map(|(i, entry)| {
                matcher.fuzzy_match(&entry.label, query).map(|score| (score, i))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        self.fuzzy_matches = scored.into_iter().map(|(_, i)| i).collect();
        if self.fuzzy_selected >= self.fuzzy_matches.len() {
            self.fuzzy_selected = self.fuzzy_matches.len().saturating_sub(1);
        }
    }

    pub fn fuzzy_input_char(&mut self, c: char) {
        self.fuzzy_query.push(c);
        self.apply_fuzzy_filter();
    }

    pub fn fuzzy_input_backspace(&mut self) {
        self.fuzzy_query.pop();
        self.apply_fuzzy_filter();
    }

    pub fn fuzzy_next(&mut self) {
        if self.fuzzy_matches.is_empty() {
            return;
        }
        self.fuzzy_selected = (self.fuzzy_selected + 1) % self.fuzzy_matches.len();
    }

    pub fn fuzzy_previous(&mut self) {
        if self.fuzzy_matches.is_empty() {
            return;
        }
        self.fuzzy_selected = if self.fuzzy_selected == 0 {
            self.fuzzy_matches.len() - 1
        } else {
            self.fuzzy_selected - 1
        };
    }

    pub fn fuzzy_select(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.fuzzy_matches.is_empty() {
            self.close_fuzzy();
            return Ok(());
        }
        let entry_idx = self.fuzzy_matches[self.fuzzy_selected];
        let entry = &self.fuzzy_entries[entry_idx];
        match entry.kind {
            FuzzyKind::Table => {
                if let Some(ti) = entry.table_index {
                    self.load_table(ti)?;
                }
            }
            FuzzyKind::Query => {
                if let Some(qi) = entry.query_index {
                    self.load_query(qi)?;
                }
            }
        }
        self.close_fuzzy();
        Ok(())
    }
}


