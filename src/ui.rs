//! User interface rendering.
//!
//! This module is responsible for drawing the TUI layout. It splits the terminal into a
//! left-hand table list and a right-hand data panel, computes column widths from the actual
//! cell contents (clamped to a maximum), and handles horizontal column scrolling when the table
//! is wider than the available space. It also provides text-truncation helpers so that oversized
//! cell values are shown with an ellipsis.

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table},
    Frame,
};

use crate::app::App;

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

    let list = List::new(items).block(Block::default().title("Tables").borders(Borders::ALL));
    frame.render_widget(list, main_layout[0]);

    // Right column: Table data
    if !app.headers.is_empty() {
        let inner_width = main_layout[1].width.saturating_sub(2); // -2 for borders

        // Compute column widths based on data
        let mut col_widths: Vec<u16> =
            app.headers.iter().map(|h| h.chars().count() as u16).collect();
        for row in &app.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_widths.len() {
                    col_widths[i] = col_widths[i].max(cell.chars().count() as u16);
                }
            }
        }
        // Clamp to max width
        for w in &mut col_widths {
            *w = (*w).min(MAX_COL_WIDTH);
        }

        // Determine visible columns based on h_scroll and available width
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

        let header_cells: Vec<Cell> = visible_headers
            .iter()
            .enumerate()
            .map(|(i, h)| {
                let width = visible_widths[i] as usize;
                let truncated = truncate_with_ellipsis(h, width);
                Cell::from(truncated)
                    .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
            })
            .collect();
        let header =
            Row::new(header_cells).style(Style::default().add_modifier(Modifier::UNDERLINED));

        let rows: Vec<Row> = app
            .rows
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

        let constraints: Vec<Constraint> =
            visible_widths.iter().map(|&w| Constraint::Length(w)).collect();

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

        let mut block = Block::default().title(title).borders(Borders::ALL);
        if app.table_focused {
            block = block
                .border_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
        }

        let table = Table::new(rows, &constraints).header(header).block(block);
        let table = if app.table_focused {
            table.highlight_style(Style::default().fg(Color::White).add_modifier(Modifier::REVERSED))
        } else {
            table
        };
        frame.render_stateful_widget(table, main_layout[1], &mut app.table_state);
    } else {
        let paragraph = ratatui::widgets::Paragraph::new("No table selected or table is empty")
            .block(Block::default().title("Data").borders(Borders::ALL));
        frame.render_widget(paragraph, main_layout[1]);
    }

    // Keybind reference bar.
    // Controls that don't change between screens are left-aligned before those that do.
    let keybinds = if app.table_focused {
        vec![
            Span::raw(" q"),
            Span::styled(": Quit ", Style::default().fg(Color::DarkGray)),
            Span::raw("Esc"),
            Span::styled(": Unfocus ", Style::default().fg(Color::DarkGray)),
            Span::raw("j/k"),
            Span::styled(": Scroll ", Style::default().fg(Color::DarkGray)),
            Span::raw("h/l"),
            Span::styled(": Scroll Left/Right", Style::default().fg(Color::DarkGray)),
        ]
    } else {
        vec![
            Span::raw(" q"),
            Span::styled(": Quit ", Style::default().fg(Color::DarkGray)),
            Span::raw("j/k"),
            Span::styled(": Navigate ", Style::default().fg(Color::DarkGray)),
            Span::raw("l"),
            Span::styled(": Focus Table", Style::default().fg(Color::DarkGray)),
        ]
    };
    let keybind_line = Line::from(keybinds);
    let keybind_bar = Paragraph::new(keybind_line);
    frame.render_widget(keybind_bar, outer_layout[1]);
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
