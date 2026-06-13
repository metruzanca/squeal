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
        .unwrap_or_default()
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
        let mut sql = format!("SELECT * FROM \"{}\"", table_name);

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
                let clause = match op {
                    FilterOp::Equals => {
                        format!("CAST(\"{}\" AS TEXT) = ${}", headers[*i], param_idx)
                    }
                    FilterOp::Contains => {
                        format!(
                            "CAST(\"{}\" AS TEXT) ILIKE '%' || ${} || '%'",
                            headers[*i], param_idx
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
            sql.push_str(&format!(" ORDER BY \"{}\" {}", headers[sort_col], dir));
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
                con.conname AS constraint_name, \
                a.attname AS from_col, \
                c.relname AS ref_table, \
                af.attname AS to_col, \
                i.ord \
             FROM pg_constraint con \
             JOIN pg_class cl ON con.conrelid = cl.oid \
             JOIN pg_namespace n ON cl.relnamespace = n.oid \
             JOIN pg_class c ON con.confrelid = c.oid \
             JOIN generate_series(1, array_length(con.conkey, 1)) AS i(ord) ON true \
             JOIN pg_attribute a ON a.attrelid = cl.oid AND a.attnum = con.conkey[i.ord] \
             JOIN pg_attribute af ON af.attrelid = c.oid AND af.attnum = con.confkey[i.ord] \
             WHERE cl.relname = $1 AND con.contype = 'f' AND n.nspname = 'public' \
             ORDER BY con.conname, i.ord",
            &[&table_name],
        )?;

        let mut fks = Vec::new();
        for row in rows {
            fks.push(ForeignKeyInfo {
                id: 0,
                seq: row.get::<_, i32>(4),
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
            "SELECT * FROM \"{}\" WHERE \"{}\" = $1 LIMIT 1",
            table_name, ref_column
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
