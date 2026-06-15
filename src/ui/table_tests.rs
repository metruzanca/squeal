#[cfg(test)]
mod tests {
    use crate::ui::table::{compute_col_widths, table_title, visible_column_range};

    #[test]
    fn test_compute_col_widths_basic() {
        let headers = vec!["id".to_string(), "name".to_string()];
        let rows = vec![
            vec!["1".to_string(), "Alice".to_string()],
            vec!["2".to_string(), "Bob".to_string()],
        ];
        let widths = compute_col_widths(&headers, &rows);
        // "id" = 2, "1" = 1, "2" = 1 -> max = 2, +1 = 3
        // "name" = 4, "Alice" = 5, "Bob" = 3 -> max = 5, +1 = 6
        assert_eq!(widths, vec![3, 6]);
    }

    #[test]
    fn test_compute_col_widths_with_long_cell() {
        let headers = vec!["a".to_string()];
        let rows = vec![vec!["very_long_cell_content".to_string()]];
        let widths = compute_col_widths(&headers, &rows);
        // "very_long_cell_content" = 22, but capped at MAX_COL_WIDTH (30) + 1 = 23
        assert_eq!(widths, vec![23]);
    }

    #[test]
    fn test_compute_col_widths_max_cap() {
        let headers = vec!["a".to_string()];
        let rows = vec![vec!["a".repeat(40)]];
        let widths = compute_col_widths(&headers, &rows);
        // 40 + 1 = 41, but capped at MAX_COL_WIDTH (30)
        assert_eq!(widths, vec![30]);
    }

    #[test]
    fn test_compute_col_widths_empty_rows() {
        let headers = vec!["id".to_string(), "name".to_string()];
        let rows: Vec<Vec<String>> = vec![];
        let widths = compute_col_widths(&headers, &rows);
        // Just header lengths + 1
        assert_eq!(widths, vec![3, 5]);
    }

    #[test]
    fn test_compute_col_widths_fewer_cells_than_headers() {
        let headers = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let rows = vec![vec!["1".to_string(), "2".to_string()]];
        let widths = compute_col_widths(&headers, &rows);
        // Only first 2 columns get data, third uses just header
        assert_eq!(widths, vec![2, 2, 2]);
    }

    #[test]
    fn test_visible_column_range_all_fit() {
        let headers = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let col_widths = vec![5, 5, 5];
        let (visible, end) = visible_column_range(0, &headers, &col_widths, 20);
        // 5 + 1 + 5 + 1 + 5 = 17 <= 20
        assert_eq!(visible, 3);
        assert_eq!(end, 3);
    }

    #[test]
    fn test_visible_column_range_partial_fit() {
        let headers = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let col_widths = vec![10, 10, 10];
        let (visible, end) = visible_column_range(0, &headers, &col_widths, 25);
        // 10 + 1 + 10 = 21 <= 25, but 10 + 1 + 10 + 1 + 10 = 32 > 25
        assert_eq!(visible, 2);
        assert_eq!(end, 2);
    }

    #[test]
    fn test_visible_column_range_with_scroll() {
        let headers = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let col_widths = vec![10, 10, 10];
        let (visible, end) = visible_column_range(1, &headers, &col_widths, 25);
        // Starting from col 1: 10 + 1 + 10 = 21 <= 25
        assert_eq!(visible, 2);
        assert_eq!(end, 3);
    }

    #[test]
    fn test_visible_column_range_at_least_one() {
        let headers = vec!["a".to_string(), "b".to_string()];
        let col_widths = vec![50, 50];
        let (visible, end) = visible_column_range(0, &headers, &col_widths, 10);
        // First col is 50 > 10, but at least 1 visible
        assert_eq!(visible, 1);
        assert_eq!(end, 1);
    }

    #[test]
    fn test_visible_column_range_empty() {
        let headers: Vec<String> = vec![];
        let col_widths: Vec<u16> = vec![];
        let (visible, end) = visible_column_range(0, &headers, &col_widths, 20);
        assert_eq!(visible, 1);
        assert_eq!(end, 0);
    }

    #[test]
    fn test_visible_column_range_scroll_past_end() {
        let headers = vec!["a".to_string(), "b".to_string()];
        let col_widths = vec![5, 5];
        let (visible, end) = visible_column_range(5, &headers, &col_widths, 20);
        // h_scroll past headers, clamped
        assert_eq!(visible, 1);
        assert_eq!(end, 2);
    }

    #[test]
    fn test_table_title_all_visible() {
        let title = table_title("Table", 5, 0, 5);
        assert_eq!(title, "Table");
    }

    #[test]
    fn test_table_title_partial() {
        let title = table_title("Table", 10, 0, 5);
        assert_eq!(title, "Table (cols 1-5 of 10)");
    }

    #[test]
    fn test_table_title_with_scroll() {
        let title = table_title("Table", 10, 3, 8);
        assert_eq!(title, "Table (cols 4-8 of 10)");
    }

    #[test]
    fn test_table_title_single_col() {
        let title = table_title("Table", 1, 0, 1);
        assert_eq!(title, "Table");
    }
}
