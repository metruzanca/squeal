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

    /// Create an in-memory demo database with a realistic multi-table schema.
    ///
    /// This is intended for integration testing of complex features such as filtering,
    /// sorting, and pagination. It is also exposed via the `--demo` CLI flag.
    pub fn in_memory_demo() -> Connection {
        let conn = Connection::open_in_memory().unwrap();

        conn.execute(
            "CREATE TABLE users (
                id INTEGER PRIMARY KEY,
                first_name TEXT NOT NULL,
                last_name TEXT NOT NULL,
                email TEXT,
                age INTEGER,
                country TEXT,
                registered_at TEXT
            )",
            [],
        )
        .unwrap();

        let first_names = [
            "Alice", "Bob", "Charlie", "Diana", "Eve", "Frank", "Grace", "Henry", "Ivy", "Jack",
            "Karen", "Leo", "Mia", "Nathan", "Olivia", "Paul", "Quinn", "Rachel", "Sam", "Tina",
        ];
        let last_names = [
            "Smith", "Johnson", "Williams", "Brown", "Jones", "Garcia", "Miller", "Davis",
            "Rodriguez", "Martinez", "Anderson", "Taylor", "Thomas", "Jackson", "White",
            "Harris", "Martin", "Thompson", "Garcia", "Clark",
        ];
        let countries = [
            "USA", "UK", "Canada", "Germany", "France", "Japan", "Australia", "Brazil",
            "India", "Spain",
        ];

        for i in 1..=100 {
            let first = first_names[i % first_names.len()];
            let last = last_names[i % last_names.len()];
            let email = format!("{}.{}@example.com", first.to_lowercase(), last.to_lowercase());
            let age = 18 + (i % 63) as i32;
            let country = countries[i % countries.len()];
            let registered_at = format!("2023-{:02}-{:02}", 1 + (i % 12), 1 + (i % 28));

            conn.execute(
                "INSERT INTO users (first_name, last_name, email, age, country, registered_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![first, last, email, age, country, registered_at],
            )
            .unwrap();
        }

        conn.execute(
            "CREATE TABLE categories (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT
            )",
            [],
        )
        .unwrap();

        let categories = [
            ("Electronics", "Gadgets and electronic devices"),
            ("Clothing", "Apparel and fashion items"),
            ("Books", "Physical and digital books"),
            ("Home", "Home and garden supplies"),
            ("Sports", "Sports and outdoor equipment"),
            ("Food", "Food and beverages"),
            ("Toys", "Toys and games"),
            ("Automotive", "Car parts and accessories"),
            ("Health", "Health and wellness products"),
            ("Music", "Musical instruments and media"),
        ];

        for (name, desc) in &categories {
            conn.execute(
                "INSERT INTO categories (name, description) VALUES (?1, ?2)",
                rusqlite::params![name, desc],
            )
            .unwrap();
        }

        conn.execute(
            "CREATE TABLE products (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                category_id INTEGER,
                price REAL NOT NULL,
                stock INTEGER,
                rating REAL,
                description TEXT,
                FOREIGN KEY (category_id) REFERENCES categories(id)
            )",
            [],
        )
        .unwrap();

        for i in 1..=200 {
            let name = format!("Product {}", i);
            let category_id = 1 + (i % 10) as i32;
            let price = 1.0 + (i as f64 * 3.7) % 999.99;
            let stock = (i % 500) as i32;
            let rating = 1.0 + (i as f64 * 0.37) % 4.0;
            let description = format!("Description for product {} in category {}", i, category_id);

            conn.execute(
                "INSERT INTO products (name, category_id, price, stock, rating, description)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![name, category_id, price, stock, rating, description],
            )
            .unwrap();
        }

        conn.execute(
            "CREATE TABLE orders (
                id INTEGER PRIMARY KEY,
                user_id INTEGER NOT NULL,
                product_id INTEGER NOT NULL,
                quantity INTEGER NOT NULL,
                status TEXT NOT NULL,
                order_date TEXT NOT NULL,
                total REAL,
                FOREIGN KEY (user_id) REFERENCES users(id),
                FOREIGN KEY (product_id) REFERENCES products(id)
            )",
            [],
        )
        .unwrap();

        let statuses = ["pending", "completed", "cancelled", "shipped", "refunded"];

        for i in 1..=1000 {
            let user_id = 1 + (i % 100) as i32;
            let product_id = 1 + (i % 200) as i32;
            let quantity = 1 + (i % 10) as i32;
            let status = statuses[i % statuses.len()];
            let order_date = format!("2024-{:02}-{:02}", 1 + (i % 12), 1 + (i % 28));
            let total = (quantity as f64) * (1.0 + (i as f64 * 3.7) % 999.99);

            conn.execute(
                "INSERT INTO orders (user_id, product_id, quantity, status, order_date, total)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![user_id, product_id, quantity, status, order_date, total],
            )
            .unwrap();
        }

        conn
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Result as SqliteResult;

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

    #[test]
    fn test_in_memory_demo() {
        let conn = TestDb::in_memory_demo();
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<SqliteResult<Vec<String>>>()
            .unwrap();
        assert_eq!(tables, vec!["categories", "orders", "products", "users"]);

        let user_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
            .unwrap();
        assert_eq!(user_count, 100);

        let order_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM orders", [], |row| row.get(0))
            .unwrap();
        assert_eq!(order_count, 1000);
    }
}
