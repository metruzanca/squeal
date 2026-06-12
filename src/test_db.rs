use std::fs;
use rusqlite::Connection;

#[allow(dead_code)]
pub struct TestDb;

#[allow(dead_code)]
impl TestDb {
    /// Create a simple test database with two tables: users and products.
    pub fn simple(path: &str) -> Connection {
        let _ = fs::remove_file(path);
        let conn = Connection::open(path).unwrap();
        conn.execute(
            "CREATE TABLE users (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                email TEXT
            )",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (id, name, email) VALUES (1, 'Alice', 'alice@example.com')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (id, name, email) VALUES (2, 'Bob', 'bob@example.com')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (id, name, email) VALUES (3, 'Charlie', NULL)",
            [],
        )
        .unwrap();

        conn.execute(
            "CREATE TABLE products (
                id INTEGER PRIMARY KEY,
                title TEXT NOT NULL,
                price REAL
            )",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO products (id, title, price) VALUES (1, 'Widget', 9.99)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO products (id, title, price) VALUES (2, 'Gadget', 19.99)",
            [],
        )
        .unwrap();
        conn
    }

    /// Create a test database with many rows to test pagination/limit behaviour.
    pub fn large(path: &str, row_count: usize) -> Connection {
        let _ = fs::remove_file(path);
        let conn = Connection::open(path).unwrap();
        conn.execute(
            "CREATE TABLE items (
                id INTEGER PRIMARY KEY,
                name TEXT,
                value REAL
            )",
            [],
        )
        .unwrap();
        for i in 1..=row_count {
            conn.execute(
                "INSERT INTO items (name, value) VALUES (?1, ?2)",
                rusqlite::params![
                    format!("item_{}", i),
                    (i as f64) * 1.5
                ],
            )
            .unwrap();
        }
        conn
    }

    /// Create a test database with many columns to test wide table rendering.
    pub fn wide(path: &str) -> Connection {
        let _ = fs::remove_file(path);
        let conn = Connection::open(path).unwrap();
        conn.execute(
            "CREATE TABLE wide_table (
                a INTEGER PRIMARY KEY,
                b TEXT,
                c REAL,
                d INTEGER,
                e TEXT,
                f REAL,
                g INTEGER,
                h TEXT,
                i REAL,
                j INTEGER
            )",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO wide_table (a, b, c, d, e, f, g, h, i, j)
             VALUES (1, 'one', 1.1, 10, 'ten', 10.1, 100, 'hundred', 100.1, 1000)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO wide_table (a, b, c, d, e, f, g, h, i, j)
             VALUES (2, 'two', 2.2, 20, 'twenty', 20.2, 200, 'two hundred', 200.2, 2000)",
            [],
        )
        .unwrap();
        conn
    }

    /// Create an empty test database with no tables.
    pub fn empty(path: &str) -> Connection {
        let _ = fs::remove_file(path);
        Connection::open(path).unwrap()
    }

    /// Create an in-memory database with the same simple schema as `simple()`.
    pub fn in_memory_simple() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE users (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                email TEXT
            )",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (id, name, email) VALUES (1, 'Alice', 'alice@example.com')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (id, name, email) VALUES (2, 'Bob', 'bob@example.com')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (id, name, email) VALUES (3, 'Charlie', NULL)",
            [],
        )
        .unwrap();

        conn.execute(
            "CREATE TABLE products (
                id INTEGER PRIMARY KEY,
                title TEXT NOT NULL,
                price REAL
            )",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO products (id, title, price) VALUES (1, 'Widget', 9.99)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO products (id, title, price) VALUES (2, 'Gadget', 19.99)",
            [],
        )
        .unwrap();
        conn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple() {
        let conn = TestDb::simple("/tmp/test_db_simple.db");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_large() {
        let conn = TestDb::large("/tmp/test_db_large.db", 150);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 150);
    }

    #[test]
    fn test_wide() {
        let conn = TestDb::wide("/tmp/test_db_wide.db");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM wide_table", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_empty() {
        let conn = TestDb::empty("/tmp/test_db_empty.db");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
}
