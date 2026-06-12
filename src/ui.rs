//! User interface rendering.
//!
//! This module is responsible for drawing the TUI layout. It splits the terminal into a
//! left-hand table list and a right-hand data panel, computes column widths from the actual
//! cell contents (clamped to a maximum), and handles horizontal column scrolling when the table
//! is wider than the available space. It also provides text-truncation helpers so that oversized
//! cell values are shown with an ellipsis.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table, TableState},
};

use crate::app::{App, FilterMode, FilterOp};

const MAX_COL_WIDTH: u16 = 30;
const COL_FG: Color = Color::DarkGray; // muted, matches control descriptions

pub fn draw(frame: &mut Frame, app: &mut App) {
    let outer_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .split(frame.size());

    let max_table_name_len = app
        .tables
        .iter()
        .map(|t| t.chars().count() as u16)
        .max()
        .unwrap_or(0);
    let left_width = (max_table_name_len + 3).max(8); // +1 padding + 2 borders, min 8 for "Tables"

    let main_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(left_width), Constraint::Fill(1)])
        .split(outer_layout[0]);

    // Left column: Table list
    let items: Vec<ListItem> = app
        .tables
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let style = if i == app.selected {
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(name.as_str()).style(style)
        })
        .collect();

    let mut list_block = Block::default().title("Tables").borders(Borders::ALL);
    if !app.table_focused {
        list_block = list_block.border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    }
    let list = List::new(items).block(list_block);
    frame.render_widget(list, main_layout[0]);

    // Right column: Table data
    if !app.headers.is_empty() {
        // Compute active filter count for filter bar height
        let active_filter_count = app.filters.iter().filter(|f| f.is_some()).count() as u16;
        let filter_bar_height = active_filter_count;
        let type_select_height = if app.filter_mode == FilterMode::TypeSelect { 1 } else { 0 };
        let value_input_height = if app.filter_mode == FilterMode::ValueInput { 1 } else { 0 };
        let _filter_area_height = filter_bar_height + type_select_height + value_input_height;

        // Compute column widths based on data
        let mut col_widths: Vec<u16> = app
            .headers
            .iter()
            .map(|h| h.chars().count() as u16)
            .collect();
        for row in &app.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_widths.len() {
                    col_widths[i] = col_widths[i].max(cell.chars().count() as u16);
                }
            }
        }
        // Reserve 1 char for sort arrow, then clamp to max width
        for w in &mut col_widths {
            *w = (*w + 1).min(MAX_COL_WIDTH);
        }

        // Determine visible columns based on h_scroll and available width
        let inner_width = main_layout[1].width.saturating_sub(2); // -2 for borders
        let spacing = 1; // default column spacing
        let total_table_width = col_widths.iter().copied().sum::<u16>()
            + spacing * (app.headers.len().saturating_sub(1) as u16);
        app.needs_h_scroll = total_table_width > inner_width;

        let mut visible_count = 0;
        let mut current_width = 0;
        for i in app.h_scroll..app.headers.len() {
            if i > app.h_scroll {
                current_width += spacing;
            }
            current_width += col_widths[i];
            if current_width > inner_width && visible_count > 0 {
                break;
            }
            visible_count += 1;
        }
        visible_count = visible_count.max(1);
        let end_col = (app.h_scroll + visible_count).min(app.headers.len());

        let visible_headers = &app.headers[app.h_scroll..end_col];
        let visible_widths = &col_widths[app.h_scroll..end_col];

        let title = if app.headers.len() > visible_count {
            format!(
                "Table: {} (cols {}-{} of {})",
                app.tables[app.selected],
                app.h_scroll + 1,
                end_col,
                app.headers.len()
            )
        } else {
            format!("Table: {}", app.tables[app.selected])
        };

        // Build and render outer block
        let mut block = Block::default().title(title).borders(Borders::ALL);
        if app.table_focused {
            block = block.border_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );
        }
        let right_inner = block.inner(main_layout[1]);
        frame.render_widget(block, main_layout[1]);

        // Build constraints for the inner layout
        let mut constraints: Vec<Constraint> = Vec::new();
        if type_select_height > 0 {
            constraints.push(Constraint::Length(type_select_height));
        }
        if filter_bar_height > 0 {
            constraints.push(Constraint::Length(filter_bar_height));
        }
        if value_input_height > 0 {
            constraints.push(Constraint::Length(value_input_height));
        }
        constraints.push(Constraint::Fill(1));

        let right_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(right_inner);

        let mut layout_idx = 0;

        // Render type select dropdown
        if app.filter_mode == FilterMode::TypeSelect {
            let type_select_area = right_layout[layout_idx];
            layout_idx += 1;
            let col_name = &app.headers[app.filter_col];
            let eq_style = if app.temp_filter_op == FilterOp::Equals {
                Style::default().fg(Color::White).add_modifier(Modifier::REVERSED)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let contains_style = if app.temp_filter_op == FilterOp::Contains {
                Style::default().fg(Color::White).add_modifier(Modifier::REVERSED)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let type_line = Line::from(vec![
                Span::raw(format!("Filter: {} | ", col_name)),
                Span::styled("equals", eq_style),
                Span::raw("  "),
                Span::styled("contains", contains_style),
            ]);
            let type_paragraph = Paragraph::new(type_line);
            frame.render_widget(type_paragraph, type_select_area);
        }

        // Render active filter bar
        if filter_bar_height > 0 {
            let filter_bar_area = right_layout[layout_idx];
            layout_idx += 1;
            let filter_lines: Vec<Line> = app
                .filters
                .iter()
                .enumerate()
                .filter_map(|(i, f)| {
                    f.as_ref().map(|(op, val)| {
                        let op_str = match op {
                            FilterOp::Equals => "=",
                            FilterOp::Contains => "~",
                        };
                        let content = format!("{}: {} {}", app.headers[i], op_str, val);
                        Line::from(Span::styled(content, Style::default().fg(Color::DarkGray)))
                    })
                })
                .collect();
            let filter_paragraph = Paragraph::new(filter_lines);
            frame.render_widget(filter_paragraph, filter_bar_area);
        }

        // Render value input
        if app.filter_mode == FilterMode::ValueInput {
            let value_input_area = right_layout[layout_idx];
            layout_idx += 1;
            let col_name = &app.headers[app.filter_col];
            let op_str = match app.temp_filter_op {
                FilterOp::Equals => "=",
                FilterOp::Contains => "~",
            };
            let prefix = format!("Filter: {} {} ", col_name, op_str);
            let value_line = Line::from(vec![
                Span::raw(&prefix),
                Span::styled(
                    &app.temp_filter_value,
                    Style::default().fg(Color::White),
                ),
            ]);
            let value_paragraph = Paragraph::new(value_line);
            frame.render_widget(value_paragraph, value_input_area);

            // Set cursor at end of value input
            let cursor_x = value_input_area.x
                + prefix.chars().count() as u16
                + app.temp_filter_value.chars().count() as u16;
            let cursor_y = value_input_area.y;
            frame.set_cursor(cursor_x, cursor_y);
        }

        // Table area (always last)
        let table_area = right_layout[layout_idx];

        // Compute page_size from table area height
        app.page_size = table_area.height.saturating_sub(1) as usize;
        let end = (app.scroll_offset + app.page_size).min(app.rows.len());

        // Build header cells with sort arrows and selection highlighting
        let header_cells: Vec<Cell> = visible_headers
            .iter()
            .enumerate()
            .map(|(i, h)| {
                let width = visible_widths[i] as usize;

                // Add sort arrow
                let mut header_text = h.clone();
                if let Some(sort_col) = app.sort_col {
                    if app.h_scroll + i == sort_col {
                        let arrow = if app.sort_asc { " ↑" } else { " ↓" };
                        header_text.push_str(arrow);
                    }
                }

                let truncated = truncate_with_ellipsis(&header_text, width);

                let is_selected = app.filter_mode == FilterMode::HeaderSelect
                    && (app.h_scroll + i) == app.filter_col;

                let mut cell_style =
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
                if is_selected {
                    cell_style = cell_style.add_modifier(Modifier::REVERSED);
                }

                Cell::from(truncated).style(cell_style)
            })
            .collect();
        let header =
            Row::new(header_cells).style(Style::default().add_modifier(Modifier::UNDERLINED));

        let rows: Vec<Row> = app.rows[app.scroll_offset..end]
            .iter()
            .map(|row_data| {
                let visible_cells = &row_data[app.h_scroll..end_col];
                let cells: Vec<Cell> = visible_cells
                    .iter()
                    .enumerate()
                    .map(|(i, text)| {
                        let width = visible_widths[i] as usize;
                        let truncated = truncate_with_ellipsis(text, width);
                        if (app.h_scroll + i) % 2 == 0 {
                            Cell::from(truncated)
                        } else {
                            Cell::from(truncated).style(Style::default().fg(COL_FG))
                        }
                    })
                    .collect();
                Row::new(cells)
            })
            .collect();

        let constraints: Vec<Constraint> = visible_widths
            .iter()
            .map(|&w| Constraint::Length(w))
            .collect();

        // Build table without block (borders are on the outer block)
        let table = Table::new(rows, &constraints).header(header);
        let table = if app.table_focused && app.filter_mode == FilterMode::None {
            table.highlight_style(
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::REVERSED),
            )
        } else {
            table
        };

        let mut render_state =
            TableState::new().with_selected(app.table_state.selected().and_then(|s| {
                if s >= app.scroll_offset && s < end {
                    Some(s - app.scroll_offset)
                } else {
                    None
                }
            }));
        frame.render_stateful_widget(table, table_area, &mut render_state);
    } else {
        let paragraph = ratatui::widgets::Paragraph::new("No table selected or table is empty")
            .block(Block::default().title("Data").borders(Borders::ALL));
        frame.render_widget(paragraph, main_layout[1]);
    }

    // Keybind reference bar.
    // Controls that don't change between screens are left-aligned before those that do.
    let keybinds = if app.modal_open {
        vec![
            Span::raw(" q"),
            Span::styled(": Quit ", Style::default().fg(Color::DarkGray)),
            Span::raw("Esc"),
            Span::styled(": Close ", Style::default().fg(Color::DarkGray)),
            Span::raw("j/k"),
            Span::styled(": Cycle ", Style::default().fg(Color::DarkGray)),
            Span::raw("h/l"),
            Span::styled(": Scroll Left/Right ", Style::default().fg(Color::DarkGray)),
            Span::raw("Enter"),
            Span::styled(": Go to Table", Style::default().fg(Color::DarkGray)),
        ]
    } else if app.table_focused {
        if app.filter_mode != FilterMode::None {
            match app.filter_mode {
                FilterMode::HeaderSelect => vec![
                    Span::raw(" q"),
                    Span::styled(": Quit ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Esc"),
                    Span::styled(": Cancel ", Style::default().fg(Color::DarkGray)),
                    Span::raw("h/l"),
                    Span::styled(": Column ", Style::default().fg(Color::DarkGray)),
                    Span::raw("j/k"),
                    Span::styled(": Sort ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Enter"),
                    Span::styled(": Add/Edit ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Del"),
                    Span::styled(": Remove", Style::default().fg(Color::DarkGray)),
                ],
                FilterMode::TypeSelect => vec![
                    Span::raw(" q"),
                    Span::styled(": Quit ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Esc"),
                    Span::styled(": Cancel ", Style::default().fg(Color::DarkGray)),
                    Span::raw("h/l/j/k"),
                    Span::styled(": Toggle ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Enter"),
                    Span::styled(": Value ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Del"),
                    Span::styled(": Remove", Style::default().fg(Color::DarkGray)),
                ],
                FilterMode::ValueInput => vec![
                    Span::raw(" q"),
                    Span::styled(": Quit ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Esc"),
                    Span::styled(": Cancel ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Enter"),
                    Span::styled(": Apply ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Del"),
                    Span::styled(": Remove", Style::default().fg(Color::DarkGray)),
                ],
                FilterMode::None => unreachable!(),
            }
        } else {
            vec![
                Span::raw(" q"),
                Span::styled(": Quit ", Style::default().fg(Color::DarkGray)),
                Span::raw("Tab"),
                Span::styled(": Table List ", Style::default().fg(Color::DarkGray)),
                Span::raw("j/k"),
                Span::styled(": Scroll ", Style::default().fg(Color::DarkGray)),
                Span::raw("PgUp/PgDn"),
                Span::styled(": Page ", Style::default().fg(Color::DarkGray)),
                Span::raw("h/l"),
                Span::styled(": Scroll Left/Right ", Style::default().fg(Color::DarkGray)),
                Span::raw("Enter"),
                Span::styled(": FK Records ", Style::default().fg(Color::DarkGray)),
                Span::raw("/"),
                Span::styled(": Filter", Style::default().fg(Color::DarkGray)),
            ]
        }
    } else {
        vec![
            Span::raw(" q"),
            Span::styled(": Quit ", Style::default().fg(Color::DarkGray)),
            Span::raw("Tab"),
            Span::styled(": View Table ", Style::default().fg(Color::DarkGray)),
            Span::raw("j/k"),
            Span::styled(": Navigate", Style::default().fg(Color::DarkGray)),
        ]
    };
    let keybind_line = Line::from(keybinds);
    let keybind_bar = Paragraph::new(keybind_line);

    // Split the keybind bar into left (keybinds) and right (?)
    let help_width = 15u16;
    let keybind_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Fill(1), Constraint::Length(help_width)])
        .split(outer_layout[1]);
    frame.render_widget(keybind_bar, keybind_layout[0]);

    let help_hint = Line::from(vec![
        Span::raw("?"),
        Span::styled(": help", Style::default().fg(Color::DarkGray)),
    ]);
    let help_paragraph = Paragraph::new(help_hint).alignment(ratatui::layout::Alignment::Right);
    frame.render_widget(help_paragraph, keybind_layout[1]);

    // Help modal overlay
    if app.help_open {
        let area = centered_rect(60, 60, frame.size());
        frame.render_widget(Clear, area);
        let help_block = Block::default()
            .title("Help")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));
        frame.render_widget(help_block, area);

        let inner = Rect::new(
            area.x + 1,
            area.y + 1,
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );

        let mut help_lines = Vec::new();
        help_lines.push(Line::from(Span::styled(
            "Global",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
        help_lines.push(Line::from("  q          : Quit"));
        help_lines.push(Line::from("  ?          : Toggle Help"));
        help_lines.push(Line::from(""));
        help_lines.push(Line::from(Span::styled(
            "Table List",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
        help_lines.push(Line::from("  j / Down   : Next table"));
        help_lines.push(Line::from("  k / Up     : Previous table"));
        help_lines.push(Line::from("  Tab / Enter: View table"));
        help_lines.push(Line::from(""));
        help_lines.push(Line::from(Span::styled(
            "Table View",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
        help_lines.push(Line::from("  j / Down   : Scroll down"));
        help_lines.push(Line::from("  k / Up     : Scroll up"));
        help_lines.push(Line::from("  h / Left   : Scroll left"));
        help_lines.push(Line::from("  l / Right  : Scroll right"));
        help_lines.push(Line::from("  PgUp/PgDn  : Page up / down"));
        help_lines.push(Line::from("  Tab        : Back to table list"));
        help_lines.push(Line::from("  Enter      : Open FK records"));
        help_lines.push(Line::from("  /          : Filter mode"));
        help_lines.push(Line::from(""));
        help_lines.push(Line::from(Span::styled(
            "Filter Mode",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
        help_lines.push(Line::from("  h / Left   : Select column"));
        help_lines.push(Line::from("  j / Down   : Sort / toggle type"));
        help_lines.push(Line::from("  k / Up     : Sort / toggle type"));
        help_lines.push(Line::from("  l / Right  : Select column"));
        help_lines.push(Line::from("  Enter      : Add/edit filter for column"));
        help_lines.push(Line::from("  Delete     : Remove existing filter"));
        help_lines.push(Line::from("  Esc        : Cancel and return"));
        help_lines.push(Line::from("  /          : Toggle filter mode"));
        help_lines.push(Line::from(""));
        help_lines.push(Line::from(Span::styled(
            "FK Records Modal",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
        help_lines.push(Line::from("  j / Down   : Next record"));
        help_lines.push(Line::from("  k / Up     : Previous record"));
        help_lines.push(Line::from("  h / Left   : Scroll left"));
        help_lines.push(Line::from("  l / Right  : Scroll right"));
        help_lines.push(Line::from("  Enter      : Go to referenced table"));
        help_lines.push(Line::from("  Esc        : Close modal"));

        // Split inner area into content (top) and footer (bottom)
        let help_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Fill(1), Constraint::Length(1)])
            .split(inner);

        let help_text = Paragraph::new(help_lines);
        frame.render_widget(help_text, help_layout[0]);

        let footer = Paragraph::new("Press <esc> to close this modal")
            .alignment(ratatui::layout::Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(footer, help_layout[1]);
    }

    // Modal overlay
    if app.modal_open {
        let area = centered_rect(80, 80, frame.size());
        frame.render_widget(Clear, area);
        let block = Block::default()
            .title("Foreign Key Records")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));
        frame.render_widget(block, area);

        // Inner area inside the modal block borders
        let inner = Rect::new(
            area.x + 1,
            area.y + 1,
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );

        if app.modal_records.is_empty() {
            let paragraph = Paragraph::new("No foreign key records found.");
            frame.render_widget(paragraph, inner);
        } else {
            // Split the modal inner area into vertical chunks for each record
            let record_count = app.modal_records.len() as u16;
            let mut constraints = Vec::new();
            for _ in 0..record_count {
                constraints.push(Constraint::Length(4)); // top border + header + row + bottom border
            }
            constraints.push(Constraint::Fill(1)); // remaining space
            let record_areas = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(inner);

            // Track whether any record needs horizontal scrolling
            let _inner_width = inner.width.saturating_sub(2);
            app.modal_needs_h_scroll = false;

            for (i, record) in app.modal_records.iter().enumerate() {
                let record_area = record_areas[i];
                let is_selected = i == app.modal_selected;
                let title = format!(
                    "{} → {} ({} = {})",
                    record.fk_column, record.table_name, record.ref_column, record.fk_value
                );
                let mut record_block = Block::default().title(title).borders(Borders::ALL);
                if is_selected {
                    record_block = record_block.border_style(
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    );
                } else {
                    record_block = record_block.border_style(Style::default().fg(Color::DarkGray));
                }
                frame.render_widget(record_block, record_area);

                // Inner area for the table (inside the record block borders)
                let table_area = Rect::new(
                    record_area.x + 1,
                    record_area.y + 1,
                    record_area.width.saturating_sub(2),
                    record_area.height.saturating_sub(2),
                );

                // Compute column widths from headers + row data
                let mut col_widths: Vec<u16> = record
                    .headers
                    .iter()
                    .map(|h| h.chars().count() as u16)
                    .collect();
                for (i, cell) in record.row.iter().enumerate() {
                    if i < col_widths.len() {
                        col_widths[i] = col_widths[i].max(cell.chars().count() as u16);
                    }
                }
                for w in &mut col_widths {
                    *w = (*w).min(MAX_COL_WIDTH);
                }

                // Determine visible columns based on modal_h_scroll and available width
                let spacing = 1;
                let total_table_width = col_widths.iter().copied().sum::<u16>()
                    + spacing * (record.headers.len().saturating_sub(1) as u16);
                let needs_scroll = total_table_width > table_area.width;
                if needs_scroll {
                    app.modal_needs_h_scroll = true;
                }

                let mut visible_count = 0;
                let mut current_width = 0;
                for j in app.modal_h_scroll..record.headers.len() {
                    if j > app.modal_h_scroll {
                        current_width += spacing;
                    }
                    current_width += col_widths[j];
                    if current_width > table_area.width && visible_count > 0 {
                        break;
                    }
                    visible_count += 1;
                }
                visible_count = visible_count.max(1);
                let end_col = (app.modal_h_scroll + visible_count).min(record.headers.len());

                let visible_headers = &record.headers[app.modal_h_scroll..end_col];
                let visible_widths = &col_widths[app.modal_h_scroll..end_col];

                // Create header row
                let header_cells: Vec<Cell> = visible_headers
                    .iter()
                    .enumerate()
                    .map(|(j, h)| {
                        let width = visible_widths[j] as usize;
                        let truncated = truncate_with_ellipsis(h, width);
                        Cell::from(truncated).style(
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        )
                    })
                    .collect();
                let header = Row::new(header_cells)
                    .style(Style::default().add_modifier(Modifier::UNDERLINED));

                // Create single data row with alternating column colors
                let visible_cells = &record.row[app.modal_h_scroll..end_col];
                let data_cells: Vec<Cell> = visible_cells
                    .iter()
                    .enumerate()
                    .map(|(j, text)| {
                        let width = visible_widths[j] as usize;
                        let truncated = truncate_with_ellipsis(text, width);
                        if (app.modal_h_scroll + j) % 2 == 0 {
                            Cell::from(truncated)
                        } else {
                            Cell::from(truncated).style(Style::default().fg(COL_FG))
                        }
                    })
                    .collect();
                let data_row = Row::new(data_cells);

                let constraints: Vec<Constraint> = visible_widths
                    .iter()
                    .map(|&w| Constraint::Length(w))
                    .collect();

                let table = Table::new(vec![data_row], &constraints).header(header);
                frame.render_widget(table, table_area);
            }
        }
    }
}

/// Compute a centered rectangle inside the given area using percentage dimensions.
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let width = area.width * percent_x / 100;
    let height = area.height * percent_y / 100;
    let x = area.width.saturating_sub(width) / 2;
    let y = area.height.saturating_sub(height) / 2;
    Rect::new(area.x + x, area.y + y, width, height)
}

pub fn truncate_with_ellipsis(s: &str, max_width: usize) -> String {
    let len = s.chars().count();
    if len <= max_width {
        s.to_string()
    } else if max_width <= 3 {
        s.chars().take(max_width).collect()
    } else {
        let mut result = String::new();
        for ch in s.chars().take(max_width - 3) {
            result.push(ch);
        }
        result.push_str("...");
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_with_ellipsis() {
        assert_eq!(truncate_with_ellipsis("hello", 10), "hello");
        assert_eq!(truncate_with_ellipsis("hello world", 5), "he...");
        assert_eq!(truncate_with_ellipsis("abc", 2), "ab");
        assert_eq!(truncate_with_ellipsis("abc", 3), "abc");
        assert_eq!(truncate_with_ellipsis("abcd", 3), "abc");
        assert_eq!(truncate_with_ellipsis("", 5), "");
    }
}
