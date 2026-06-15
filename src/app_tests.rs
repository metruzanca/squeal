#[cfg(test)]
mod tests {
    use crate::app::App;
    use crate::app::FilterMode;
    use crate::driver::sqlite::SQLiteDriver;
    use crate::driver::FilterOp;
    use crate::test_db;

    impl App {
        pub fn clear_filter_for_col(&mut self, col: usize) {
            if col < self.filters.len() {
                self.filters[col] = None;
                let _ = self.apply_filters_and_sort();
            }
        }
    }

    #[test]
    fn test_app_navigation() {
        let path = "/tmp/squeal_test_nav.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        assert_eq!(app.selected_sidebar, 0);
        assert_eq!(app.tables[app.selected_sidebar], "products");

        app.next();
        assert_eq!(app.selected_sidebar, 1);
        assert_eq!(app.tables[app.selected_sidebar], "users");
        assert_eq!(app.headers, vec!["id", "name", "email"]);
        assert_eq!(app.rows.len(), 3);

        app.next();
        assert_eq!(app.selected_sidebar, 0);
        assert_eq!(app.tables[app.selected_sidebar], "products");

        app.previous();
        assert_eq!(app.selected_sidebar, 1);
        assert_eq!(app.tables[app.selected_sidebar], "users");
    }

    #[test]
    fn test_focus_and_unfocus() {
        let path = "/tmp/squeal_test_focus.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        assert!(!app.table_focused);
        assert_eq!(app.table_state.selected(), None);

        app.focus_table();
        assert!(app.table_focused);
        assert_eq!(app.table_state.selected(), Some(0));

        app.scroll_table_down();
        assert_eq!(app.table_state.selected(), Some(1));

        app.scroll_table_up();
        assert_eq!(app.table_state.selected(), Some(0));

        // Horizontal scrolling is blocked when needs_h_scroll is false
        app.h_scroll_right();
        assert_eq!(app.h_scroll, 0);

        app.needs_h_scroll = true;
        app.h_scroll_right();
        assert_eq!(app.h_scroll, 1);

        app.h_scroll_left();
        assert_eq!(app.h_scroll, 0);

        app.unfocus_table();
        assert!(!app.table_focused);
        assert_eq!(app.table_state.selected(), None);
        assert_eq!(app.h_scroll, 0);
    }

    #[test]
    fn test_navigation_blocked_when_focused() {
        let path = "/tmp/squeal_test_nav_block.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();
        assert!(app.table_focused);
        assert_eq!(app.selected_sidebar, 0);

        app.next();
        assert_eq!(app.selected_sidebar, 0); // should not change
        app.previous();
        assert_eq!(app.selected_sidebar, 0); // should not change
    }

    #[test]
    fn test_fetch_more_rows() {
        let path = "/tmp/squeal_test_fetch_more.db";
        test_db::TestDb::large(path, 250);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        assert_eq!(app.rows.len(), 100);
        assert!(app.has_more_rows);

        app.fetch_more_rows().unwrap();
        assert_eq!(app.rows.len(), 200);
        assert!(app.has_more_rows);

        app.fetch_more_rows().unwrap();
        assert_eq!(app.rows.len(), 250);
        assert!(!app.has_more_rows);

        // Fetching again when no more rows should be a no-op
        app.fetch_more_rows().unwrap();
        assert_eq!(app.rows.len(), 250);
        assert!(!app.has_more_rows);
    }

    #[test]
    fn test_scroll_table_down_fetches_more() {
        let path = "/tmp/squeal_test_scroll_fetch.db";
        test_db::TestDb::large(path, 250);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();

        // Scroll to bottom of first batch
        for _ in 0..99 {
            app.scroll_table_down();
        }
        assert_eq!(app.table_state.selected(), Some(99));
        assert_eq!(app.rows.len(), 100);
        assert!(app.has_more_rows);

        // One more scroll should trigger fetching
        app.scroll_table_down();
        assert_eq!(app.table_state.selected(), Some(100));
        assert_eq!(app.rows.len(), 200);
        assert!(app.has_more_rows);

        // Scroll to bottom of second batch
        for _ in 0..99 {
            app.scroll_table_down();
        }
        assert_eq!(app.table_state.selected(), Some(199));
        assert_eq!(app.rows.len(), 200);

        // Scroll to trigger final fetch
        app.scroll_table_down();
        assert_eq!(app.table_state.selected(), Some(200));
        assert_eq!(app.rows.len(), 250);
        assert!(!app.has_more_rows);

        // Keep scrolling to the end
        for _ in 0..49 {
            app.scroll_table_down();
        }
        assert_eq!(app.table_state.selected(), Some(249));
        assert_eq!(app.rows.len(), 250);

        // Scroll past the end should stay at the bottom
        app.scroll_table_down();
        assert_eq!(app.table_state.selected(), Some(249));
        assert_eq!(app.rows.len(), 250);
    }

    #[test]
    fn test_small_table_no_fetch() {
        let path = "/tmp/squeal_test_small_no_fetch.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();

        // products has 2 rows, so has_more_rows should be false
        assert!(!app.has_more_rows);

        app.scroll_table_down();
        assert_eq!(app.table_state.selected(), Some(1));
        app.scroll_table_down();
        assert_eq!(app.table_state.selected(), Some(1)); // stays at bottom
        assert_eq!(app.rows.len(), 2);
    }

    #[test]
    fn test_page_down_scrolls_view_and_preserves_visual_position() {
        let path = "/tmp/squeal_test_page_down.db";
        test_db::TestDb::large(path, 250);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();
        app.page_size = 10;

        // visual position 0 preserved
        app.page_down();
        assert_eq!(app.scroll_offset, 10);
        assert_eq!(app.table_state.selected(), Some(10));

        app.page_down();
        assert_eq!(app.scroll_offset, 20);
        assert_eq!(app.table_state.selected(), Some(20));
    }

    #[test]
    fn test_page_up_preserves_visual_position() {
        let path = "/tmp/squeal_test_page_up.db";
        test_db::TestDb::large(path, 250);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();
        app.page_size = 10;
        app.scroll_offset = 50;
        app.table_state.select(Some(55));

        app.page_up();
        assert_eq!(app.scroll_offset, 40);
        assert_eq!(app.table_state.selected(), Some(45)); // visual pos 5 preserved
    }

    #[test]
    fn test_page_down_clamps_cursor_on_partial_page() {
        // 15 rows with page_size=10: pages 0-9, 10-14
        let path = "/tmp/squeal_test_page_down_clamp.db";
        test_db::TestDb::large(path, 15);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();
        app.page_size = 10;
        app.scroll_offset = 0;
        app.table_state.select(Some(9)); // visual pos 9

        app.page_down();
        assert_eq!(app.scroll_offset, 10);
        assert_eq!(app.table_state.selected(), Some(14)); // clamped to last row
    }

    #[test]
    fn test_page_up_at_top_is_noop() {
        let path = "/tmp/squeal_test_page_up_clamp.db";
        test_db::TestDb::large(path, 250);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();
        app.page_size = 10;
        app.scroll_offset = 0;
        app.table_state.select(Some(5));

        app.page_up();
        assert_eq!(app.scroll_offset, 0);
        assert_eq!(app.table_state.selected(), Some(5)); // no change
    }

    #[test]
    fn test_page_down_to_last_page_preserves_visual_position() {
        let path = "/tmp/squeal_test_small_final.db";
        test_db::TestDb::large(path, 25);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();
        app.page_size = 10;

        // Page 1: 0-9
        app.page_down();
        assert_eq!(app.scroll_offset, 10);
        assert_eq!(app.table_state.selected(), Some(10));

        // Page 2: 10-19
        app.page_down();
        assert_eq!(app.scroll_offset, 20);
        assert_eq!(app.table_state.selected(), Some(20)); // visual pos 0 preserved
    }

    #[test]
    fn test_page_up_from_last_page_preserves_visual_position() {
        let path = "/tmp/squeal_test_up_from_bottom.db";
        test_db::TestDb::large(path, 25);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();
        app.page_size = 10;

        // Page 1: 0-9
        app.page_down();
        assert_eq!(app.scroll_offset, 10);
        assert_eq!(app.table_state.selected(), Some(10));

        // Page 2: 10-19
        app.page_down();
        assert_eq!(app.scroll_offset, 20);
        assert_eq!(app.table_state.selected(), Some(20));

        // Page 1: visual pos 0 preserved
        app.page_up();
        assert_eq!(app.scroll_offset, 10);
        assert_eq!(app.table_state.selected(), Some(10));
    }

    #[test]
    fn test_page_down_from_last_page_is_noop() {
        let path = "/tmp/squeal_test_page_down_noop.db";
        test_db::TestDb::large(path, 25);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();
        app.page_size = 10;

        app.page_down(); // 10
        app.page_down(); // 20 (last page)
        app.page_down(); // should be no-op
        assert_eq!(app.scroll_offset, 20);
        assert_eq!(app.table_state.selected(), Some(20));
    }

    #[test]
    fn test_page_up_from_partial_page_preserves_visual_position() {
        // 15 rows with page_size=10: pages 0-9, 10-14
        let path = "/tmp/squeal_test_up_from_partial.db";
        test_db::TestDb::large(path, 15);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();
        app.page_size = 10;

        app.page_down(); // offset 10
        app.table_state.select(Some(12)); // visual pos 2
        app.page_up(); // offset 0
        assert_eq!(app.scroll_offset, 0);
        assert_eq!(app.table_state.selected(), Some(2)); // visual pos 2 preserved
    }

    #[test]
    fn test_scroll_table_down_keeps_cursor_visible() {
        let path = "/tmp/squeal_test_cursor_vis.db";
        test_db::TestDb::large(path, 250);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();
        app.page_size = 10;

        // Move cursor to row 15
        for _ in 0..15 {
            app.scroll_table_down();
        }
        assert_eq!(app.table_state.selected(), Some(15));
        assert_eq!(app.scroll_offset, 6); // window shifted to keep cursor visible
    }

    #[test]
    fn test_open_modal_fetches_fk_records() {
        let conn = test_db::TestDb::in_memory_demo();
        let mut app = App::new(Box::new(SQLiteDriver::from_connection(conn))).unwrap();
        // Navigate to orders table (should be index 2 after sorting: categories, orders, products, users)
        app.selected_sidebar = app.tables.iter().position(|t| t == "orders").unwrap();
        app.load_table(app.selected_sidebar).unwrap();
        app.focus_table();
        assert_eq!(app.table_state.selected(), Some(0));

        app.open_modal().unwrap();
        assert!(app.modal_open);
        assert!(!app.modal_records.is_empty());

        // orders row 0 has user_id = 2 and product_id = 2 (i=1 in the loop)
        let user_record = app
            .modal_records
            .iter()
            .find(|r| r.table_name == "users");
        assert!(user_record.is_some());
        let user_record = user_record.unwrap();
        assert_eq!(user_record.fk_column, "user_id");
        assert_eq!(user_record.fk_value, "2");
        assert_eq!(user_record.headers, vec!["id", "first_name", "last_name", "email", "age", "country", "registered_at"]);
        assert_eq!(user_record.row[0], "2"); // id
        assert_eq!(user_record.row[1], "Charlie"); // first_name

        let product_record = app
            .modal_records
            .iter()
            .find(|r| r.table_name == "products");
        assert!(product_record.is_some());
        let product_record = product_record.unwrap();
        assert_eq!(product_record.fk_column, "product_id");
        assert_eq!(product_record.fk_value, "2");
    }

    #[test]
    fn test_close_modal() {
        let conn = test_db::TestDb::in_memory_demo();
        let mut app = App::new(Box::new(SQLiteDriver::from_connection(conn))).unwrap();
        app.selected_sidebar = app.tables.iter().position(|t| t == "orders").unwrap();
        app.load_table(app.selected_sidebar).unwrap();
        app.focus_table();
        app.open_modal().unwrap();
        assert!(app.modal_open);
        assert!(!app.modal_records.is_empty());

        app.close_modal();
        assert!(!app.modal_open);
        assert!(app.modal_records.is_empty());
    }

    #[test]
    fn test_unfocus_table_closes_modal() {
        let conn = test_db::TestDb::in_memory_demo();
        let mut app = App::new(Box::new(SQLiteDriver::from_connection(conn))).unwrap();
        app.selected_sidebar = app.tables.iter().position(|t| t == "orders").unwrap();
        app.load_table(app.selected_sidebar).unwrap();
        app.focus_table();
        app.open_modal().unwrap();
        assert!(app.modal_open);

        app.unfocus_table();
        assert!(!app.modal_open);
        assert!(app.modal_records.is_empty());
    }

    #[test]
    fn test_load_table_closes_modal() {
        let conn = test_db::TestDb::in_memory_demo();
        let mut app = App::new(Box::new(SQLiteDriver::from_connection(conn))).unwrap();
        app.selected_sidebar = app.tables.iter().position(|t| t == "orders").unwrap();
        app.load_table(app.selected_sidebar).unwrap();
        app.focus_table();
        app.open_modal().unwrap();
        assert!(app.modal_open);

        // Load a different table
        app.selected_sidebar = app.tables.iter().position(|t| t == "users").unwrap();
        app.load_table(app.selected_sidebar).unwrap();
        assert!(!app.modal_open);
        assert!(app.modal_records.is_empty());
    }

    #[test]
    fn test_open_modal_no_fks() {
        let conn = test_db::TestDb::in_memory_demo();
        let mut app = App::new(Box::new(SQLiteDriver::from_connection(conn))).unwrap();
        // users table has no foreign keys - should show row details instead
        app.selected_sidebar = app.tables.iter().position(|t| t == "users").unwrap();
        app.load_table(app.selected_sidebar).unwrap();
        app.focus_table();
        app.open_modal().unwrap();
        assert!(app.modal_open);
        assert_eq!(app.modal_records.len(), 1);
        assert_eq!(app.modal_records[0].table_name, "users");
        assert_eq!(app.modal_records[0].headers, vec!["id", "first_name", "last_name", "email", "age", "country", "registered_at"]);
    }

    // Filter mode tests

    #[test]
    fn test_filter_mode_toggle() {
        let path = "/tmp/squeal_test_filter_toggle.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();

        // Cannot toggle when not focused
        app.toggle_filter_mode();
        assert_eq!(app.filter_mode, FilterMode::None);

        app.focus_table();
        app.toggle_filter_mode();
        assert_eq!(app.filter_mode, FilterMode::HeaderSelect);
        assert_eq!(app.filter_col, 0);

        app.toggle_filter_mode();
        assert_eq!(app.filter_mode, FilterMode::None);
    }

    #[test]
    fn test_filter_mode_blocked_when_not_focused() {
        let path = "/tmp/squeal_test_filter_block.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        assert!(!app.table_focused);

        app.move_filter_col_right();
        assert_eq!(app.filter_col, 0);
        app.cycle_sort_order();
        assert_eq!(app.sort_col, None);
    }

    #[test]
    fn test_cycle_sort_order() {
        let path = "/tmp/squeal_test_sort.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();
        app.toggle_filter_mode();

        // No sort -> Asc on col 0
        app.cycle_sort_order();
        assert_eq!(app.sort_col, Some(0));
        assert!(app.sort_asc);

        // Asc -> Desc
        app.cycle_sort_order();
        assert_eq!(app.sort_col, Some(0));
        assert!(!app.sort_asc);

        // Desc -> None
        app.cycle_sort_order();
        assert_eq!(app.sort_col, None);

        // Move to col 1 and sort
        app.move_filter_col_right();
        app.cycle_sort_order();
        assert_eq!(app.sort_col, Some(1));
        assert!(app.sort_asc);

        // Move back to col 0, should set new sort
        app.move_filter_col_left();
        app.cycle_sort_order();
        assert_eq!(app.sort_col, Some(0));
        assert!(app.sort_asc);
    }

    #[test]
    fn test_filter_input() {
        let path = "/tmp/squeal_test_filter_input.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        app.enter_filter_for_col();

        assert_eq!(app.filter_mode, FilterMode::TypeSelect);
        // id is a number column, default is Equals, toggle goes to NotEquals
        app.toggle_filter_type();
        assert_eq!(app.temp_filter_op, FilterOp::NotEquals);
        app.move_to_value_input();
        assert_eq!(app.filter_mode, FilterMode::ValueInput);

        app.filter_input_char('W');
        app.filter_input_char('i');
        assert_eq!(app.temp_filter_value, "Wi");

        app.filter_input_backspace();
        assert_eq!(app.temp_filter_value, "W");
    }

    #[test]
    fn test_filter_navigation() {
        let path = "/tmp/squeal_test_filter_nav.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();
        app.toggle_filter_mode();

        assert_eq!(app.filter_mode, FilterMode::HeaderSelect);
        app.move_filter_col_right();
        assert_eq!(app.filter_col, 1);
        app.move_filter_col_left();
        assert_eq!(app.filter_col, 0);

        app.enter_filter_for_col();
        assert_eq!(app.filter_mode, FilterMode::TypeSelect);
        // id is a number column, default is Equals, toggle goes to NotEquals
        app.toggle_filter_type();
        assert_eq!(app.temp_filter_op, FilterOp::NotEquals);
        app.move_to_value_input();
        assert_eq!(app.filter_mode, FilterMode::ValueInput);

        app.cancel_filter_mode();
        assert_eq!(app.filter_mode, FilterMode::None);
    }

    #[test]
    fn test_filter_applies_and_sorts() {
        let path = "/tmp/squeal_test_filter_apply.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        app.move_filter_col_right(); // move to title column
        app.enter_filter_for_col();
        // title is a string column, default is Contains
        app.move_to_value_input();
        app.filter_input_char('W');
        app.apply_filter();

        assert_eq!(app.filter_mode, FilterMode::None);
        assert_eq!(app.rows.len(), 1);
        assert_eq!(app.rows[0], vec!["1", "Widget", "9.99"]);
    }

    #[test]
    fn test_filter_empty_returns_all() {
        let path = "/tmp/squeal_test_filter_empty.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        app.apply_filters_and_sort().unwrap();

        assert_eq!(app.rows.len(), 2);
    }

    #[test]
    fn test_sort_ascending() {
        let path = "/tmp/squeal_test_sort_asc.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        app.move_filter_col_right(); // select title column
        app.cycle_sort_order(); // asc
        app.apply_filters_and_sort().unwrap();

        assert_eq!(app.rows.len(), 2);
        assert_eq!(app.rows[0], vec!["2", "Gadget", "19.99"]);
        assert_eq!(app.rows[1], vec!["1", "Widget", "9.99"]);
    }

    #[test]
    fn test_sort_descending() {
        let path = "/tmp/squeal_test_sort_desc.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        app.move_filter_col_right(); // select title column
        app.cycle_sort_order(); // asc
        app.cycle_sort_order(); // desc
        app.apply_filters_and_sort().unwrap();

        assert_eq!(app.rows.len(), 2);
        assert_eq!(app.rows[0], vec!["1", "Widget", "9.99"]);
        assert_eq!(app.rows[1], vec!["2", "Gadget", "19.99"]);
    }

    #[test]
    fn test_filter_and_sort_combined() {
        let path = "/tmp/squeal_test_filter_sort.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        app.move_filter_col_right(); // move to title column
        app.enter_filter_for_col();
        // title is a string column, default is Contains
        app.move_to_value_input();
        app.filter_input_char('e');
        app.apply_filter();

        // Both Widget and Gadget contain 'e'
        assert_eq!(app.rows.len(), 2);

        // Now sort by title descending
        app.toggle_filter_mode();
        app.move_filter_col_right(); // select title column
        app.cycle_sort_order(); // asc
        app.cycle_sort_order(); // desc
        app.apply_filters_and_sort().unwrap();

        assert_eq!(app.rows[0], vec!["1", "Widget", "9.99"]);
        assert_eq!(app.rows[1], vec!["2", "Gadget", "19.99"]);
    }

    #[test]
    fn test_filter_case_insensitive() {
        let path = "/tmp/squeal_test_filter_ci.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        app.move_filter_col_right(); // move to title column
        app.enter_filter_for_col();
        // title is a string column, default is Contains
        app.move_to_value_input();
        app.filter_input_char('w');
        app.apply_filter();

        assert_eq!(app.rows.len(), 1);
        assert_eq!(app.rows[0], vec!["1", "Widget", "9.99"]);
    }

    #[test]
    fn test_unfocus_clears_filter_mode() {
        let path = "/tmp/squeal_test_unfocus_filter.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        assert_eq!(app.filter_mode, FilterMode::HeaderSelect);

        app.unfocus_table();
        assert_eq!(app.filter_mode, FilterMode::None);
        assert!(!app.table_focused);
    }

    #[test]
    fn test_filter_on_multiple_columns() {
        let path = "/tmp/squeal_test_filter_multi.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        app.enter_filter_for_col(); // filter on id column
        app.move_to_value_input();
        app.filter_input_char('1');
        app.apply_filter();

        // Now add another filter on price using Contains
        app.toggle_filter_mode();
        app.move_filter_col_right();
        app.move_filter_col_right(); // price column
        app.enter_filter_for_col();
        // price is a number column, default is Equals, toggle twice to get Contains
        app.toggle_filter_type();
        app.toggle_filter_type();
        app.move_to_value_input();
        app.filter_input_char('9');
        app.apply_filter();

        assert_eq!(app.rows.len(), 1);
        assert_eq!(app.rows[0], vec!["1", "Widget", "9.99"]);
    }

    #[test]
    fn test_filter_equals() {
        let path = "/tmp/squeal_test_filter_equals.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        app.move_filter_col_right(); // title column
        app.enter_filter_for_col();
        // title is a string column, default is Contains, toggle once to Equals
        app.toggle_filter_type();
        app.move_to_value_input();
        app.filter_input_char('W');
        app.filter_input_char('i');
        app.filter_input_char('d');
        app.filter_input_char('g');
        app.filter_input_char('e');
        app.filter_input_char('t');
        app.apply_filter();

        assert_eq!(app.rows.len(), 1);
        assert_eq!(app.rows[0], vec!["1", "Widget", "9.99"]);
    }

    #[test]
    fn test_clear_filter() {
        let path = "/tmp/squeal_test_clear_filter.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        app.move_filter_col_right(); // title column
        app.enter_filter_for_col();
        // title is a string column, default is Contains
        app.move_to_value_input();
        app.filter_input_char('W');
        app.apply_filter();

        assert_eq!(app.rows.len(), 1);

        app.clear_filter_for_col(1);
        assert_eq!(app.rows.len(), 2);
    }

    #[test]
    fn test_delete_current_filter() {
        let path = "/tmp/squeal_test_delete_filter.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        app.move_filter_col_right(); // title column
        app.enter_filter_for_col();
        // title is a string column, default is Contains
        app.move_to_value_input();
        app.filter_input_char('W');
        app.apply_filter();

        assert_eq!(app.rows.len(), 1);
        assert_eq!(app.filters[1], Some((FilterOp::Contains, "W".to_string())));

        // Delete from HeaderSelect mode
        app.toggle_filter_mode();
        app.move_filter_col_right(); // move back to title column
        app.delete_current_filter();

        assert_eq!(app.filter_mode, FilterMode::None);
        assert_eq!(app.filters[1], None);
        assert_eq!(app.rows.len(), 2);
    }

    #[test]
    fn test_edit_existing_filter() {
        let path = "/tmp/squeal_test_edit_filter.db";
        test_db::TestDb::simple(path);
        let mut app = App::new(Box::new(SQLiteDriver::new(path).unwrap())).unwrap();
        app.focus_table();
        app.toggle_filter_mode();
        app.move_filter_col_right(); // title column
        app.enter_filter_for_col();
        // title is a string column, default is Contains
        app.move_to_value_input();
        app.filter_input_char('W');
        app.apply_filter();

        assert_eq!(app.rows.len(), 1);
        assert_eq!(app.filters[1], Some((FilterOp::Contains, "W".to_string())));

        // Re-enter filter mode on same column - should edit existing filter
        app.toggle_filter_mode();
        app.move_filter_col_right(); // title column
        app.enter_filter_for_col();

        // Should have pre-populated with existing filter
        assert_eq!(app.temp_filter_op, FilterOp::Contains);
        assert_eq!(app.temp_filter_value, "W");
        assert_eq!(app.filter_mode, FilterMode::TypeSelect);

        // Change to Equals and update value
        app.toggle_filter_type(); // switch to Equals
        app.move_to_value_input();
        app.filter_input_backspace(); // remove 'W'
        app.filter_input_char('G');
        app.filter_input_char('a');
        app.filter_input_char('d');
        app.filter_input_char('g');
        app.filter_input_char('e');
        app.filter_input_char('t');
        app.apply_filter();

        assert_eq!(app.filter_mode, FilterMode::None);
        assert_eq!(app.filters[1], Some((FilterOp::Equals, "Gadget".to_string())));
        assert_eq!(app.rows.len(), 1);
        assert_eq!(app.rows[0], vec!["2", "Gadget", "19.99"]);
    }
}
