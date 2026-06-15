#[cfg(test)]
mod tests {
    use std::error::Error;

    use rusqlite::Connection;

    use crate::driver::{DbDriver, FilterOp};
    use crate::driver::sqlite::SQLiteDriver;

    fn setup_db() -> Result<Connection, Box<dyn Error>> {
        let conn = Connection::open_in_memory()?;
        conn.execute(
            "CREATE TABLE users (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                email TEXT
            )",
            [],
        )?;
        conn.execute(
            "INSERT INTO users (id, name, email) VALUES (1, 'Alice', 'alice@example.com')",
            [],
        )?;
        conn.execute(
            "INSERT INTO users (id, name, email) VALUES (2, 'Bob', 'bob@example.com')",
            [],
        )?;
        conn.execute(
            "INSERT INTO users (id, name, email) VALUES (3, 'Charlie', NULL)",
            [],
        )?;

        conn.execute(
            "CREATE TABLE products (
                id INTEGER PRIMARY KEY,
                title TEXT NOT NULL,
                price REAL
            )",
            [],
        )?;
        conn.execute(
            "INSERT INTO products (id, title, price) VALUES (1, 'Widget', 9.99)",
            [],
        )?;
        conn.execute(
            "INSERT INTO products (id, title, price) VALUES (2, 'Gadget', 19.99)",
            [],
        )?;
        Ok(conn)
    }

    fn setup_fk_db() -> Result<Connection, Box<dyn Error>> {
        let conn = Connection::open_in_memory()?;
        conn.execute(
            "CREATE TABLE categories (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE items (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                category_id INTEGER REFERENCES categories(id)
            )",
            [],
        )?;
        conn.execute(
            "INSERT INTO categories (name) VALUES ('Electronics')",
            [],
        )?;
        conn.execute(
            "INSERT INTO categories (name) VALUES ('Books')",
            [],
        )?;
        conn.execute(
            "INSERT INTO items (name, category_id) VALUES ('Phone', 1)",
            [],
        )?;
        conn.execute(
            "INSERT INTO items (name, category_id) VALUES ('Novel', 2)",
            [],
        )?;
        Ok(conn)
    }

    #[test]
    fn test_list_tables() {
        let conn = setup_db().unwrap();
        let mut driver = SQLiteDriver::from_connection(conn);

        let tables = driver.list_tables().unwrap();
        assert_eq!(tables, vec!["products", "users"]);
    }

    #[test]
    fn test_table_columns() {
        let conn = setup_db().unwrap();
        let mut driver = SQLiteDriver::from_connection(conn);

        let cols = driver.table_columns("users").unwrap();
        assert_eq!(cols, vec!["id", "name", "email"]);
    }

    #[test]
    fn test_fetch_rows() {
        let conn = setup_db().unwrap();
        let mut driver = SQLiteDriver::from_connection(conn);

        let headers = vec!["id".to_string(), "name".to_string(), "email".to_string()];
        let rows = driver.fetch_rows("users", &headers, &[None, None, None], None, true, 0, 100).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], vec!["1", "Alice", "alice@example.com"]);
        assert_eq!(rows[1], vec!["2", "Bob", "bob@example.com"]);
        assert_eq!(rows[2], vec!["3", "Charlie", ""]);
    }

    #[test]
    fn test_fetch_rows_with_filter() {
        let conn = setup_db().unwrap();
        let mut driver = SQLiteDriver::from_connection(conn);

        let headers = vec!["id".to_string(), "name".to_string(), "email".to_string()];
        let filters = vec![None, Some((FilterOp::Equals, "Alice".to_string())), None];
        let rows = driver.fetch_rows("users", &headers, &filters, None, true, 0, 100).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], vec!["1", "Alice", "alice@example.com"]);
    }

    #[test]
    fn test_fetch_rows_with_contains_filter() {
        let conn = setup_db().unwrap();
        let mut driver = SQLiteDriver::from_connection(conn);

        let headers = vec!["id".to_string(), "name".to_string(), "email".to_string()];
        let filters = vec![None, Some((FilterOp::Contains, "li".to_string())), None];
        let rows = driver.fetch_rows("users", &headers, &filters, None, true, 0, 100).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], vec!["1", "Alice", "alice@example.com"]);
        assert_eq!(rows[1], vec!["3", "Charlie", ""]);
    }

    #[test]
    fn test_fetch_rows_with_sort() {
        let conn = setup_db().unwrap();
        let mut driver = SQLiteDriver::from_connection(conn);

        let headers = vec!["id".to_string(), "name".to_string(), "email".to_string()];
        let rows = driver.fetch_rows("users", &headers, &[None, None, None], Some(1), false, 0, 100).unwrap();
        // Sorted by name descending: Charlie, Bob, Alice
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], vec!["3", "Charlie", ""]);
        assert_eq!(rows[1], vec!["2", "Bob", "bob@example.com"]);
        assert_eq!(rows[2], vec!["1", "Alice", "alice@example.com"]);
    }

    #[test]
    fn test_fetch_rows_with_limit_offset() {
        let conn = setup_db().unwrap();
        let mut driver = SQLiteDriver::from_connection(conn);

        let headers = vec!["id".to_string(), "name".to_string(), "email".to_string()];
        let rows = driver.fetch_rows("users", &headers, &[None, None, None], None, true, 1, 1).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], vec!["2", "Bob", "bob@example.com"]);
    }

    #[test]
    fn test_run_query() {
        let conn = setup_db().unwrap();
        let mut driver = SQLiteDriver::from_connection(conn);

        let (headers, rows) = driver.run_query("SELECT * FROM users WHERE id = 1").unwrap();
        assert_eq!(headers, vec!["id", "name", "email"]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], vec!["1", "Alice", "alice@example.com"]);
    }

    #[test]
    fn test_run_query_error() {
        let conn = setup_db().unwrap();
        let mut driver = SQLiteDriver::from_connection(conn);

        let (headers, rows) = driver.run_query("SELECT * FROM nonexistent_table").unwrap();
        assert_eq!(headers, vec!["Error"]);
        assert_eq!(rows.len(), 1);
        assert!(rows[0][0].contains("nonexistent") || rows[0][0].contains("error"), "Expected error message, got: {}", rows[0][0]);
    }

    #[test]
    fn test_get_foreign_keys() {
        let conn = setup_fk_db().unwrap();
        let mut driver = SQLiteDriver::from_connection(conn);

        let fks = driver.get_foreign_keys("items").unwrap();
        assert_eq!(fks.len(), 1);
        assert_eq!(fks[0].from, "category_id");
        assert_eq!(fks[0].table, "categories");
        assert_eq!(fks[0].to, "id");
    }

    #[test]
    fn test_get_foreign_keys_no_fks() {
        let conn = setup_fk_db().unwrap();
        let mut driver = SQLiteDriver::from_connection(conn);

        let fks = driver.get_foreign_keys("categories").unwrap();
        assert!(fks.is_empty());
    }

    #[test]
    fn test_fetch_related_record() {
        let conn = setup_fk_db().unwrap();
        let mut driver = SQLiteDriver::from_connection(conn);

        let (headers, row) = driver.fetch_related_record("categories", "id", "1").unwrap().unwrap();
        assert_eq!(headers, vec!["id", "name"]);
        assert_eq!(row, vec!["1", "Electronics"]);
    }

    #[test]
    fn test_fetch_related_record_not_found() {
        let conn = setup_fk_db().unwrap();
        let mut driver = SQLiteDriver::from_connection(conn);

        let result = driver.fetch_related_record("categories", "id", "999").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_null_value_roundtrip() {
        let conn = setup_db().unwrap();
        let mut driver = SQLiteDriver::from_connection(conn);

        let headers = vec!["id".to_string(), "name".to_string(), "email".to_string()];
        let rows = driver.fetch_rows("users", &headers, &[None, None, None], None, true, 0, 100).unwrap();
        assert_eq!(rows[2], vec!["3", "Charlie", ""]);
    }

    #[test]
    fn test_real_value_roundtrip() {
        let conn = setup_db().unwrap();
        let mut driver = SQLiteDriver::from_connection(conn);

        let headers = vec!["id".to_string(), "title".to_string(), "price".to_string()];
        let rows = driver.fetch_rows("products", &headers, &[None, None, None], None, true, 0, 100).unwrap();
        assert_eq!(rows[0], vec!["1", "Widget", "9.99"]);
        assert_eq!(rows[1], vec!["2", "Gadget", "19.99"]);
    }
}
