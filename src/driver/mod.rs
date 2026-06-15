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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sqlite_value_to_string_null() {
        assert_eq!(sqlite_value_to_string(&rusqlite::types::Value::Null), "");
    }

    #[test]
    fn test_sqlite_value_to_string_integer() {
        assert_eq!(sqlite_value_to_string(&rusqlite::types::Value::Integer(42)), "42");
        assert_eq!(sqlite_value_to_string(&rusqlite::types::Value::Integer(-7)), "-7");
    }

    #[test]
    fn test_sqlite_value_to_string_real() {
        assert_eq!(sqlite_value_to_string(&rusqlite::types::Value::Real(3.14)), "3.14");
    }

    #[test]
    fn test_sqlite_value_to_string_text() {
        assert_eq!(sqlite_value_to_string(&rusqlite::types::Value::Text("hello".to_string())), "hello");
    }

    #[test]
    fn test_sqlite_value_to_string_blob() {
        let blob = rusqlite::types::Value::Blob(vec![104, 101, 108, 108, 111]); // "hello"
        assert_eq!(sqlite_value_to_string(&blob), "hello");
    }

    #[test]
    fn test_sqlite_value_to_string_blob_invalid_utf8() {
        let blob = rusqlite::types::Value::Blob(vec![0xff, 0xfe]);
        assert_eq!(sqlite_value_to_string(&blob), "\u{FFFD}\u{FFFD}");
    }

    #[test]
    fn test_collect_active_filters_empty() {
        let filters: Vec<Option<(FilterOp, String)>> = vec![None, None, None];
        let result = collect_active_filters(&filters);
        assert!(result.is_empty());
    }

    #[test]
    fn test_collect_active_filters_single() {
        let filters = vec![
            None,
            Some((FilterOp::Equals, "Alice".to_string())),
            None,
        ];
        let result = collect_active_filters(&filters);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, 1);
        assert_eq!(result[0].1, &FilterOp::Equals);
        assert_eq!(result[0].2, "Alice");
    }

    #[test]
    fn test_collect_active_filters_multiple() {
        let filters = vec![
            Some((FilterOp::Equals, "1".to_string())),
            Some((FilterOp::Contains, "test".to_string())),
        ];
        let result = collect_active_filters(&filters);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, 0);
        assert_eq!(result[0].1, &FilterOp::Equals);
        assert_eq!(result[0].2, "1");
        assert_eq!(result[1].0, 1);
        assert_eq!(result[1].1, &FilterOp::Contains);
        assert_eq!(result[1].2, "test");
    }

    #[test]
    fn test_collect_active_filters_all_none() {
        let filters: Vec<Option<(FilterOp, String)>> = vec![];
        let result = collect_active_filters(&filters);
        assert!(result.is_empty());
    }
}
