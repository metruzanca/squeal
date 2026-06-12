use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table, TableState},
};

use crate::app::FilterMode;

const MAX_COL_WIDTH: u16 = 30;
const COL_FG: Color = Color::DarkGray;

pub fn compute_col_widths(headers: &[String], rows: &[Vec<String>]) -> Vec<u16> {
    let mut widths: Vec<u16> = headers.iter().map(|h| h.chars().count() as u16).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(cell.chars().count() as u16);
            }
        }
    }
    for w in &mut widths {
        *w = (*w + 1).min(MAX_COL_WIDTH);
    }
    widths
}

pub fn visible_column_range(
    h_scroll: usize,
    headers: &[String],
    col_widths: &[u16],
    available_width: u16,
) -> (usize, usize) {
    let spacing = 1;
    let mut visible_count = 0;
    let mut current_width = 0;
    for i in h_scroll..headers.len() {
        if i > h_scroll {
            current_width += spacing;
        }
        current_width += col_widths[i];
        if current_width > available_width && visible_count > 0 {
            break;
        }
        visible_count += 1;
    }
    visible_count = visible_count.max(1);
    let end_col = (h_scroll + visible_count).min(headers.len());
    (visible_count, end_col)
}

pub fn table_title(base: &str, total_cols: usize, visible_start: usize, visible_end: usize) -> String {
    if total_cols > (visible_end - visible_start) {
        format!(
            "{} (cols {}-{} of {})",
            base,
            visible_start + 1,
            visible_end,
            total_cols
        )
    } else {
        base.to_string()
    }
}

/// Render a table widget with a surrounding block.
/// Returns `(needs_h_scroll, page_size)`.
pub fn render_table_with_block(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    headers: &[String],
    rows: &[Vec<String>],
    h_scroll: usize,
    scroll_offset: usize,
    table_state: &mut TableState,
    highlight: bool,
    filter_mode: FilterMode,
    filter_col: usize,
    sort_col: Option<usize>,
    sort_asc: bool,
) -> (bool, usize) {
    let col_widths = compute_col_widths(headers, rows);
    let inner_width = area.width.saturating_sub(2);
    let (_, end_col) = visible_column_range(h_scroll, headers, &col_widths, inner_width);
    let needs_h_scroll = headers.len() > (end_col - h_scroll);

    let display_title = table_title(title, headers.len(), h_scroll, end_col);

    let mut block = Block::default().title(display_title).borders(Borders::ALL);
    if highlight {
        block = block.border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    }
    let table_area = block.inner(area);
    frame.render_widget(block, area);

    let page_size = render_table_widget(
        frame,
        table_area,
        headers,
        rows,
        h_scroll,
        scroll_offset,
        table_state,
        highlight,
        filter_mode,
        filter_col,
        sort_col,
        sort_asc,
        &col_widths,
        end_col,
    );

    (needs_h_scroll, page_size)
}

/// Render a table widget without a surrounding block.
/// Returns `page_size`.
pub fn render_table_widget(
    frame: &mut Frame,
    area: Rect,
    headers: &[String],
    rows: &[Vec<String>],
    h_scroll: usize,
    scroll_offset: usize,
    table_state: &mut TableState,
    highlight: bool,
    filter_mode: FilterMode,
    filter_col: usize,
    sort_col: Option<usize>,
    sort_asc: bool,
    col_widths: &[u16],
    end_col: usize,
) -> usize {
    let visible_headers = &headers[h_scroll..end_col];
    let visible_widths = &col_widths[h_scroll..end_col];

    let page_size = area.height.saturating_sub(1) as usize;
    let end = (scroll_offset + page_size).min(rows.len());

    let header_cells: Vec<Cell> = visible_headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let width = visible_widths[i] as usize;
            let mut header_text = h.clone();
            if let Some(sort_col) = sort_col {
                if h_scroll + i == sort_col {
                    let arrow = if sort_asc { " ↑" } else { " ↓" };
                    header_text.push_str(arrow);
                }
            }
            let truncated = super::helpers::truncate_with_ellipsis(&header_text, width);
            let is_selected = filter_mode == FilterMode::HeaderSelect && (h_scroll + i) == filter_col;
            let mut cell_style = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
            if is_selected {
                cell_style = cell_style.add_modifier(Modifier::REVERSED);
            }
            Cell::from(truncated).style(cell_style)
        })
        .collect();
    let header = Row::new(header_cells).style(Style::default().add_modifier(Modifier::UNDERLINED));

    let display_rows: Vec<Row> = rows[scroll_offset..end]
        .iter()
        .map(|row_data| {
            let visible_cells = &row_data[h_scroll..end_col];
            let cells: Vec<Cell> = visible_cells
                .iter()
                .enumerate()
                .map(|(i, text)| {
                    let width = visible_widths[i] as usize;
                    let truncated = super::helpers::truncate_with_ellipsis(text, width);
                    if (h_scroll + i) % 2 == 0 {
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

    let table = Table::new(display_rows, &constraints).header(header);
    let table = if highlight && filter_mode == FilterMode::None {
        table.highlight_style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::REVERSED),
        )
    } else {
        table
    };

    let mut render_state = TableState::new().with_selected(table_state.selected().and_then(|s| {
        if s >= scroll_offset && s < end {
            Some(s - scroll_offset)
        } else {
            None
        }
    }));
    frame.render_stateful_widget(table, area, &mut render_state);

    page_size
}

/// Render a simple table for modal records (no sorting, filtering, or highlighting).
/// Returns `needs_h_scroll`.
pub fn render_modal_table(
    frame: &mut Frame,
    area: Rect,
    headers: &[String],
    row: &[String],
    h_scroll: usize,
) -> bool {
    let col_widths = compute_col_widths(headers, &[row.to_vec()]);
    let inner_width = area.width.saturating_sub(2);
    let (visible_count, end_col) = visible_column_range(h_scroll, headers, &col_widths, inner_width);
    let needs_h_scroll = visible_count < headers.len();

    let visible_headers = &headers[h_scroll..end_col];
    let visible_widths = &col_widths[h_scroll..end_col];

    let header_cells: Vec<Cell> = visible_headers
        .iter()
        .enumerate()
        .map(|(j, h)| {
            let width = visible_widths[j] as usize;
            let truncated = super::helpers::truncate_with_ellipsis(h, width);
            Cell::from(truncated).style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
        })
        .collect();
    let header = Row::new(header_cells).style(Style::default().add_modifier(Modifier::UNDERLINED));

    let visible_cells = &row[h_scroll..end_col];
    let data_cells: Vec<Cell> = visible_cells
        .iter()
        .enumerate()
        .map(|(j, text)| {
            let width = visible_widths[j] as usize;
            let truncated = super::helpers::truncate_with_ellipsis(text, width);
            if (h_scroll + j) % 2 == 0 {
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
    frame.render_widget(table, area);

    needs_h_scroll
}
