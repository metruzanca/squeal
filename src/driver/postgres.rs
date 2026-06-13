use std::error::Error;

use postgres::NoTls;

use crate::driver::{DbDriver, FilterOp, ForeignKeyInfo};

pub struct PostgresDriver {
    client: postgres::Client,
}

impl PostgresDriver {
    pub fn new(connection_string: &str) -> Result<Self, Box<dyn Error>> {
        let client = postgres::Client::connect(connection_string, NoTls)?;
        Ok(Self { client })
    }
}

/// Escape double quotes in a PostgreSQL identifier so it can be safely
/// embedded in a double-quoted identifier.
fn escape_ident(s: &str) -> String {
    s.replace('"', "\"\"")
}

fn row_value_to_string(row: &postgres::Row, idx: usize) -> String {
    row.try_get::<_, Option<String>>(idx)
        .ok()
        .flatten()
        .or_else(|| {
            row.try_get::<_, Option<&str>>(idx)
                .ok()
                .flatten()
                .map(|s| s.to_string())
        })
        .or_else(|| {
            row.try_get::<_, Option<i32>>(idx)
                .ok()
                .flatten()
                .map(|v| v.to_string())
        })
        .or_else(|| {
            row.try_get::<_, Option<i64>>(idx)
                .ok()
                .flatten()
                .map(|v| v.to_string())
        })
        .or_else(|| {
            row.try_get::<_, Option<f64>>(idx)
                .ok()
                .flatten()
                .map(|v| v.to_string())
        })
        .or_else(|| {
            row.try_get::<_, Option<f32>>(idx)
                .ok()
                .flatten()
                .map(|v| v.to_string())
        })
        .or_else(|| {
            row.try_get::<_, Option<bool>>(idx)
                .ok()
                .flatten()
                .map(|v| v.to_string())
        })
        .or_else(|| {
            row.try_get::<_, Option<uuid::Uuid>>(idx)
                .ok()
                .flatten()
                .map(|v| v.to_string())
        })
        .or_else(|| {
            row.try_get::<_, Option<chrono::NaiveDateTime>>(idx)
                .ok()
                .flatten()
                .map(|v| v.to_string())
        })
        .or_else(|| {
            row.try_get::<_, Option<chrono::NaiveDate>>(idx)
                .ok()
                .flatten()
                .map(|v| v.to_string())
        })
        .or_else(|| {
            row.try_get::<_, Option<chrono::DateTime<chrono::Utc>>>(idx)
                .ok()
                .flatten()
                .map(|v| v.to_string())
        })
        .or_else(|| {
            row.try_get::<_, Option<serde_json::Value>>(idx)
                .ok()
                .flatten()
                .map(|v| v.to_string())
        })
        .or_else(|| {
            row.try_get::<_, Option<Vec<u8>>>(idx)
                .ok()
                .flatten()
                .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
        })
        .unwrap_or_else(|| String::new())
}

impl DbDriver for PostgresDriver {
    fn list_tables(&mut self) -> Result<Vec<String>, Box<dyn Error>> {
        let rows = self.client.query(
            "SELECT table_name FROM information_schema.tables \
             WHERE table_schema = 'public' AND table_type = 'BASE TABLE' \
             ORDER BY table_name",
            &[],
        )?;
        let tables = rows.iter().map(|row| row.get::<_, String>(0)).collect();
        Ok(tables)
    }

    fn table_columns(&mut self, table_name: &str) -> Result<Vec<String>, Box<dyn Error>> {
        let rows = self.client.query(
            "SELECT column_name FROM information_schema.columns \
             WHERE table_name = $1 AND table_schema = 'public' \
             ORDER BY ordinal_position",
            &[&table_name],
        )?;
        let columns = rows.iter().map(|row| row.get::<_, String>(0)).collect();
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
        let mut sql = format!("SELECT * FROM \"{}\"", escape_ident(table_name));

        let active_filters: Vec<(usize, &FilterOp, &String)> = filters
            .iter()
            .enumerate()
            .filter_map(|(i, f)| f.as_ref().map(|(op, val)| (i, op, val)))
            .collect();

        let mut params: Vec<String> = Vec::new();

        if !active_filters.is_empty() {
            let mut where_clauses = Vec::new();
            for (i, op, _val) in &active_filters {
                let param_idx = params.len() + 1;
                let col = escape_ident(&headers[*i]);
                let clause = match op {
                    FilterOp::Equals => {
                        format!("CAST(\"{}\" AS TEXT) = ${}", col, param_idx)
                    }
                    FilterOp::Contains => {
                        format!(
                            "CAST(\"{}\" AS TEXT) ILIKE '%' || ${} || '%'",
                            col, param_idx
                        )
                    }
                };
                where_clauses.push(clause);
                params.push((*_val).clone());
            }
            sql.push_str(&format!(" WHERE {}", where_clauses.join(" AND ")));
        }

        if let Some(sort_col) = sort_col {
            let dir = if sort_asc { "ASC" } else { "DESC" };
            let col = escape_ident(&headers[sort_col]);
            sql.push_str(&format!(" ORDER BY \"{}\" {}", col, dir));
        }

        sql.push_str(&format!(" LIMIT {} OFFSET {}", limit, offset));

        let stmt = self.client.prepare(&sql)?;
        let col_count = headers.len();

        let param_refs: Vec<&(dyn postgres::types::ToSql + Sync)> = params
            .iter()
            .map(|s| s as &(dyn postgres::types::ToSql + Sync))
            .collect();

        let rows = self.client.query(&stmt, &param_refs)?;

        let result: Vec<Vec<String>> = rows
            .iter()
            .map(|row| {
                let mut values = Vec::with_capacity(col_count);
                for i in 0..col_count {
                    values.push(row_value_to_string(row, i));
                }
                values
            })
            .collect();

        Ok(result)
    }

    fn run_query(&mut self, sql: &str) -> Result<(Vec<String>, Vec<Vec<String>>), Box<dyn Error>> {
        let mut tx = self.client.transaction()?;

        let stmt = match tx.prepare(sql) {
            Ok(stmt) => stmt,
            Err(e) => return Ok((vec!["Error".to_string()], vec![vec![e.to_string()]])),
        };

        let headers: Vec<String> = stmt.columns().iter().map(|c| c.name().to_string()).collect();
        let col_count = headers.len();

        let rows = match tx.query(&stmt, &[]) {
            Ok(rows) => rows,
            Err(e) => return Ok((vec!["Error".to_string()], vec![vec![e.to_string()]])),
        };

        let result: Vec<Vec<String>> = rows
            .iter()
            .map(|row| {
                let mut values = Vec::with_capacity(col_count);
                for i in 0..col_count {
                    values.push(row_value_to_string(row, i));
                }
                values
            })
            .collect();

        Ok((headers, result))
    }

    fn get_foreign_keys(&mut self, table_name: &str) -> Result<Vec<ForeignKeyInfo>, Box<dyn Error>> {
        let rows = self.client.query(
            "SELECT \
                con.oid AS con_oid, \
                a.attname AS from_col, \
                c.relname AS ref_table, \
                af.attname AS to_col, \
                u.ord \
             FROM pg_constraint con \
             JOIN pg_class cl ON con.conrelid = cl.oid \
             JOIN pg_namespace n ON cl.relnamespace = n.oid \
             JOIN pg_class c ON con.confrelid = c.oid \
             CROSS JOIN LATERAL UNNEST(con.conkey, con.confkey) \
                 WITH ORDINALITY AS u(local_num, ref_num, ord) \
             JOIN pg_attribute a ON a.attrelid = cl.oid AND a.attnum = u.local_num \
             JOIN pg_attribute af ON af.attrelid = c.oid AND af.attnum = u.ref_num \
             WHERE cl.relname = $1 AND con.contype = 'f' AND n.nspname = 'public' \
             ORDER BY con.conname, u.ord",
            &[&table_name],
        )?;

        let mut fks = Vec::new();
        for row in rows {
            fks.push(ForeignKeyInfo {
                id: row.get::<_, i32>(0),
                seq: (row.get::<_, i64>(4) - 1) as i32,
                from: row.get::<_, String>(1),
                table: row.get::<_, String>(2),
                to: row.get::<_, String>(3),
            });
        }
        Ok(fks)
    }

    fn fetch_related_record(
        &mut self,
        table_name: &str,
        ref_column: &str,
        fk_value: &str,
    ) -> Result<Option<(Vec<String>, Vec<String>)>, Box<dyn Error>> {
        let rows = self.client.query(
            "SELECT column_name FROM information_schema.columns \
             WHERE table_name = $1 AND table_schema = 'public' \
             ORDER BY ordinal_position",
            &[&table_name],
        )?;
        let headers: Vec<String> = rows.iter().map(|row| row.get::<_, String>(0)).collect();

        let query = format!(
            "SELECT * FROM \"{}\" WHERE CAST(\"{}\" AS TEXT) = $1 LIMIT 1",
            escape_ident(table_name),
            escape_ident(ref_column)
        );
        let stmt = self.client.prepare(&query)?;
        let rows = self.client.query(&stmt, &[&fk_value])?;
        let col_count = headers.len();

        if let Some(row) = rows.first() {
            let mut values = Vec::with_capacity(col_count);
            for i in 0..col_count {
                values.push(row_value_to_string(row, i));
            }
            Ok(Some((headers, values)))
        } else {
            Ok(None)
        }
    }
}
