#[cfg(test)]
mod tests {
    use std::error::Error;

    use testcontainers::core::ContainerPort;
    use testcontainers::runners::SyncRunner;
    use testcontainers::ImageExt;
    use testcontainers::GenericImage;

    use crate::driver::{DbDriver, FilterOp};
    use crate::driver::postgres::PostgresDriver;

    fn start_postgres() -> Result<(String, testcontainers::Container<GenericImage>), Box<dyn Error>> {
        let container = GenericImage::new("postgres", "16")
            .with_exposed_port(ContainerPort::Tcp(5432))
            .with_env_var("POSTGRES_USER", "test")
            .with_env_var("POSTGRES_PASSWORD", "test")
            .with_env_var("POSTGRES_DB", "test")
            .start()?;

        // Wait for postgres to be ready
        std::thread::sleep(std::time::Duration::from_secs(3));

        let port = container.get_host_port_ipv4(ContainerPort::Tcp(5432))?;
        let url = format!("postgres://test:test@localhost:{}/test", port);
        Ok((url, container))
    }

    fn setup_schema(driver: &mut PostgresDriver) -> Result<(), Box<dyn Error>> {
        driver.execute(
            "CREATE TABLE users (
                id SERIAL PRIMARY KEY,
                name TEXT NOT NULL,
                email TEXT
            )",
        )?;
        driver.execute(
            "CREATE TABLE products (
                id SERIAL PRIMARY KEY,
                title TEXT NOT NULL,
                price REAL
            )",
        )?;
        driver.execute(
            "INSERT INTO users (name, email) VALUES ('Alice', 'alice@example.com')",
        )?;
        driver.execute(
            "INSERT INTO users (name, email) VALUES ('Bob', 'bob@example.com')",
        )?;
        driver.execute(
            "INSERT INTO users (name, email) VALUES ('Charlie', NULL)",
        )?;
        driver.execute(
            "INSERT INTO products (title, price) VALUES ('Widget', 9.99)",
        )?;
        driver.execute(
            "INSERT INTO products (title, price) VALUES ('Gadget', 19.99)",
        )?;
        Ok(())
    }

    fn setup_fk_schema(driver: &mut PostgresDriver) -> Result<(), Box<dyn Error>> {
        driver.execute(
            "CREATE TABLE categories (
                id SERIAL PRIMARY KEY,
                name TEXT NOT NULL
            )",
        )?;
        driver.execute(
            "CREATE TABLE items (
                id SERIAL PRIMARY KEY,
                name TEXT NOT NULL,
                category_id INTEGER REFERENCES categories(id)
            )",
        )?;
        driver.execute(
            "INSERT INTO categories (name) VALUES ('Electronics')",
        )?;
        driver.execute(
            "INSERT INTO categories (name) VALUES ('Books')",
        )?;
        driver.execute(
            "INSERT INTO items (name, category_id) VALUES ('Phone', 1)",
        )?;
        driver.execute(
            "INSERT INTO items (name, category_id) VALUES ('Novel', 2)",
        )?;
        Ok(())
    }

    #[test]
    fn test_list_tables() {
        let (url, _container) = start_postgres().unwrap();
        let mut driver = PostgresDriver::new(&url).unwrap();
        setup_schema(&mut driver).unwrap();

        let tables = driver.list_tables().unwrap();
        assert!(tables.contains(&"users".to_string()));
        assert!(tables.contains(&"products".to_string()));
    }

    #[test]
    fn test_table_columns() {
        let (url, _container) = start_postgres().unwrap();
        let mut driver = PostgresDriver::new(&url).unwrap();
        setup_schema(&mut driver).unwrap();

        let cols = driver.table_columns("users").unwrap();
        assert_eq!(cols, vec!["id", "name", "email"]);
    }

    #[test]
    fn test_fetch_rows() {
        let (url, _container) = start_postgres().unwrap();
        let mut driver = PostgresDriver::new(&url).unwrap();
        setup_schema(&mut driver).unwrap();

        let headers = vec!["id".to_string(), "name".to_string(), "email".to_string()];
        let rows = driver.fetch_rows("users", &headers, &[None, None, None], None, true, 0, 100).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], vec!["1", "Alice", "alice@example.com"]);
        assert_eq!(rows[1], vec!["2", "Bob", "bob@example.com"]);
        assert_eq!(rows[2], vec!["3", "Charlie", ""]);
    }

    #[test]
    fn test_fetch_rows_with_filter() {
        let (url, _container) = start_postgres().unwrap();
        let mut driver = PostgresDriver::new(&url).unwrap();
        setup_schema(&mut driver).unwrap();

        let headers = vec!["id".to_string(), "name".to_string(), "email".to_string()];
        let filters = vec![None, Some((FilterOp::Equals, "Alice".to_string())), None];
        let rows = driver.fetch_rows("users", &headers, &filters, None, true, 0, 100).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], vec!["1", "Alice", "alice@example.com"]);
    }

    #[test]
    fn test_fetch_rows_with_sort() {
        let (url, _container) = start_postgres().unwrap();
        let mut driver = PostgresDriver::new(&url).unwrap();
        setup_schema(&mut driver).unwrap();

        let headers = vec!["id".to_string(), "name".to_string(), "email".to_string()];
        let rows = driver.fetch_rows("users", &headers, &[None, None, None], Some(1), false, 0, 100).unwrap();
        // Sorted by name descending: Charlie, Bob, Alice
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], vec!["3", "Charlie", ""]);
        assert_eq!(rows[1], vec!["2", "Bob", "bob@example.com"]);
        assert_eq!(rows[2], vec!["1", "Alice", "alice@example.com"]);
    }

    #[test]
    fn test_run_query() {
        let (url, _container) = start_postgres().unwrap();
        let mut driver = PostgresDriver::new(&url).unwrap();
        setup_schema(&mut driver).unwrap();

        let (headers, rows) = driver.run_query("SELECT * FROM users WHERE id = 1").unwrap();
        assert_eq!(headers, vec!["id", "name", "email"]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], vec!["1", "Alice", "alice@example.com"]);
    }

    #[test]
    fn test_run_query_error() {
        let (url, _container) = start_postgres().unwrap();
        let mut driver = PostgresDriver::new(&url).unwrap();
        setup_schema(&mut driver).unwrap();

        let (headers, rows) = driver.run_query("SELECT * FROM nonexistent_table").unwrap();
        assert_eq!(headers, vec!["Error"]);
        assert_eq!(rows.len(), 1);
        assert!(rows[0][0].contains("nonexistent") || rows[0][0].contains("error"), "Expected error message, got: {}", rows[0][0]);
    }

    #[test]
    fn test_get_foreign_keys() {
        let (url, _container) = start_postgres().unwrap();
        let mut driver = PostgresDriver::new(&url).unwrap();
        setup_fk_schema(&mut driver).unwrap();

        let fks = driver.get_foreign_keys("items").unwrap();
        assert_eq!(fks.len(), 1);
        assert_eq!(fks[0].from, "category_id");
        assert_eq!(fks[0].table, "categories");
        assert_eq!(fks[0].to, "id");
    }

    #[test]
    fn test_get_foreign_keys_no_fks() {
        let (url, _container) = start_postgres().unwrap();
        let mut driver = PostgresDriver::new(&url).unwrap();
        setup_fk_schema(&mut driver).unwrap();

        let fks = driver.get_foreign_keys("categories").unwrap();
        assert!(fks.is_empty());
    }

    #[test]
    fn test_fetch_related_record() {
        let (url, _container) = start_postgres().unwrap();
        let mut driver = PostgresDriver::new(&url).unwrap();
        setup_fk_schema(&mut driver).unwrap();

        let (headers, row) = driver.fetch_related_record("categories", "id", "1").unwrap().unwrap();
        assert_eq!(headers, vec!["id", "name"]);
        assert_eq!(row, vec!["1", "Electronics"]);
    }

    #[test]
    fn test_fetch_related_record_not_found() {
        let (url, _container) = start_postgres().unwrap();
        let mut driver = PostgresDriver::new(&url).unwrap();
        setup_fk_schema(&mut driver).unwrap();

        let result = driver.fetch_related_record("categories", "id", "999").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_fetch_related_record_with_uuid() {
        let (url, _container) = start_postgres().unwrap();
        let mut driver = PostgresDriver::new(&url).unwrap();

        driver.execute(
            "CREATE TABLE docs (id UUID PRIMARY KEY, name TEXT)",
        ).unwrap();
        driver.execute(
            "CREATE TABLE refs (id SERIAL PRIMARY KEY, doc_id UUID REFERENCES docs(id))",
        ).unwrap();
        driver.execute(
            "INSERT INTO docs (id, name) VALUES ('550e8400-e29b-41d4-a716-446655440000', 'test')",
        ).unwrap();
        driver.execute(
            "INSERT INTO refs (doc_id) VALUES ('550e8400-e29b-41d4-a716-446655440000')",
        ).unwrap();

        let (headers, row) = driver.fetch_related_record("docs", "id", "550e8400-e29b-41d4-a716-446655440000").unwrap().unwrap();
        assert_eq!(headers, vec!["id", "name"]);
        assert_eq!(row, vec!["550e8400-e29b-41d4-a716-446655440000", "test"]);
    }
}
