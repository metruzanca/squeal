use std::error::Error;

/// Convert a SQLite value to a string representation.
pub fn sqlite_value_to_string(value: &rusqlite::types::Value) -> String {
    match value {
        rusqlite::types::Value::Null => String::new(),
        rusqlite::types::Value::Integer(v) => v.to_string(),
        rusqlite::types::Value::Real(v) => v.to_string(),
        rusqlite::types::Value::Text(v) => v.clone(),
        rusqlite::types::Value::Blob(v) => String::from_utf8_lossy(v).to_string(),
    }
}

/// Collect active filters from the filters slice into a vector of (column_index, op, value).
pub fn collect_active_filters(
    filters: &[Option<(FilterOp, String)>],
) -> Vec<(usize, &FilterOp, &String)> {
    filters
        .iter()
        .enumerate()
        .filter_map(|(i, f)| f.as_ref().map(|(op, val)| (i, op, val)))
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub enum FilterOp {
    Equals,
    Contains,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ForeignKeyInfo {
    pub id: i32,
    pub seq: i32,
    pub table: String,
    pub from: String,
    pub to: String,
}

pub trait DbDriver {
    fn list_tables(&mut self) -> Result<Vec<String>, Box<dyn Error>>;
    fn table_columns(&mut self, table_name: &str) -> Result<Vec<String>, Box<dyn Error>>;
    fn fetch_rows(
        &mut self,
        table_name: &str,
        headers: &[String],
        filters: &[Option<(FilterOp, String)>],
        sort_col: Option<usize>,
        sort_asc: bool,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<Vec<String>>, Box<dyn Error>>;
    fn run_query(&mut self, sql: &str) -> Result<(Vec<String>, Vec<Vec<String>>), Box<dyn Error>>;
    fn get_foreign_keys(&mut self, table_name: &str) -> Result<Vec<ForeignKeyInfo>, Box<dyn Error>>;
    fn fetch_related_record(
        &mut self,
        table_name: &str,
        ref_column: &str,
        fk_value: &str,
    ) -> Result<Option<(Vec<String>, Vec<String>)>, Box<dyn Error>>;
}

pub mod sqlite;
pub mod postgres;

#[cfg(test)]
pub mod postgres_tests;

#[cfg(test)]
pub mod sqlite_tests;
