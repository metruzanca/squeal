use std::error::Error;

use rusqlite::{Connection, Result as SqliteResult};

use crate::driver::{DbDriver, FilterOp, ForeignKeyInfo};

pub struct SQLiteDriver {
    conn: Connection,
}

impl SQLiteDriver {
    pub fn new(path: &str) -> Result<Self, Box<dyn Error>> {
        let conn = Connection::open(path)?;
        Ok(Self { conn })
    }

    pub fn from_connection(conn: Connection) -> Self {
        Self { conn }
    }
}

impl DbDriver for SQLiteDriver {
    fn list_tables(&mut self) -> Result<Vec<String>, Box<dyn Error>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")?;
        let tables = stmt
            .query_map([], |row| row.get(0))?
            .collect::<SqliteResult<Vec<String>>>()?;
        Ok(tables)
    }

    fn table_columns(&mut self, table_name: &str) -> Result<Vec<String>, Box<dyn Error>> {
        let mut stmt = self
            .conn
            .prepare(&format!("PRAGMA table_info(\"{}\")", table_name))?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<SqliteResult<Vec<String>>>()?;
        Ok(columns)
    }

    fn fetch_rows(
        &mut self,
        table_name: &str,
        headers: &[String],
        filters: &[Option<(FilterOp, String)>],
        sort_col: Option<usize>,
        sort_asc: bool,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<Vec<String>>, Box<dyn Error>> {
        let mut sql = format!("SELECT * FROM \"{}\"", table_name);

        let active_filters: Vec<(usize, &FilterOp, &String)> = filters
            .iter()
            .enumerate()
            .filter_map(|(i, f)| f.as_ref().map(|(op, val)| (i, op, val)))
            .collect();

        if !active_filters.is_empty() {
            let mut where_clauses = Vec::new();
            for (i, op, _val) in &active_filters {
                let clause = match op {
                    FilterOp::Equals => {
                        format!("CAST(\"{}\" AS TEXT) = ?", headers[*i])
                    }
                    FilterOp::Contains => {
                        format!(
                            "LOWER(CAST(\"{}\" AS TEXT)) LIKE LOWER('%' || ? || '%')",
                            headers[*i]
                        )
                    }
                };
                where_clauses.push(clause);
            }
            sql.push_str(&format!(" WHERE {}", where_clauses.join(" AND ")));
        }

        if let Some(sort_col) = sort_col {
            let dir = if sort_asc { "ASC" } else { "DESC" };
            sql.push_str(&format!(" ORDER BY \"{}\" {}", headers[sort_col], dir));
        }

        sql.push_str(&format!(" LIMIT {} OFFSET {}", limit, offset));

        let mut stmt = self.conn.prepare(&sql)?;
        let col_count = headers.len();

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

    fn run_query(&mut self, sql: &str) -> Result<(Vec<String>, Vec<Vec<String>>), Box<dyn Error>> {
        let tx = self.conn.transaction()?;
        let result = match tx.prepare(sql) {
            Ok(mut stmt) => {
                let col_count = stmt.column_count();
                let headers: Vec<String> =
                    stmt.column_names().iter().map(|s| s.to_string()).collect();
                match stmt.query_map([], |row| {
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
                }) {
                    Ok(mapped) => match mapped.collect::<SqliteResult<Vec<Vec<String>>>>() {
                        Ok(rows) => (headers, rows),
                        Err(e) => (vec!["Error".to_string()], vec![vec![e.to_string()]]),
                    },
                    Err(e) => (vec!["Error".to_string()], vec![vec![e.to_string()]]),
                }
            }
            Err(e) => (vec!["Error".to_string()], vec![vec![e.to_string()]]),
        };
        Ok(result)
    }

    fn get_foreign_keys(&mut self, table_name: &str) -> Result<Vec<ForeignKeyInfo>, Box<dyn Error>> {
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

    fn fetch_related_record(
        &mut self,
        table_name: &str,
        ref_column: &str,
        fk_value: &str,
    ) -> Result<Option<(Vec<String>, Vec<String>)>, Box<dyn Error>> {
        let headers: Vec<String> = {
            let mut stmt = self
                .conn
                .prepare(&format!("PRAGMA table_info(\"{}\")", table_name))?;
            stmt.query_map([], |row| row.get::<_, String>(1))?
                .collect::<SqliteResult<Vec<String>>>()?
        };

        let query = format!(
            "SELECT * FROM \"{}\" WHERE \"{}\" = ? LIMIT 1",
            table_name, ref_column
        );
        let mut stmt = self.conn.prepare(&query)?;
        let col_count = headers.len();
        let mut rows = stmt.query_map([fk_value.to_string()], |r| {
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

        if let Some(Ok(row)) = rows.next() {
            Ok(Some((headers, row)))
        } else {
            Ok(None)
        }
    }
}
