use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use crate::app::{App, FilterMode};

pub mod filters;
pub mod helpers;
pub mod table;

use filters::{render_filter_bar, render_type_select, render_value_input};
use helpers::{centered_rect, cursor_line_col};
use table::{
    compute_col_widths, render_modal_table, render_table_widget, render_table_with_block,
    visible_column_range,
};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let outer_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), Constraint::Length(1)])
        .split(frame.area());

    let max_table_name_len = app
        .tables
        .iter()
        .map(|t| t.chars().count() as u16)
        .max()
        .unwrap_or(0);
    let max_query_name_len = app
        .queries
        .iter()
        .map(|q| q.name.chars().count() as u16)
        .max()
        .unwrap_or(0);
    let left_width = (max_table_name_len.max(max_query_name_len) + 3).max(8);

    let main_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(left_width), Constraint::Fill(1)])
        .split(outer_layout[0]);

    // Left column: unified sidebar
    let mut items: Vec<ListItem> = Vec::new();
    for (i, name) in app.tables.iter().enumerate() {
        let style = if i == app.selected_sidebar {
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        items.push(ListItem::new(name.as_str()).style(style));
    }
    if !app.tables.is_empty() && !app.queries.is_empty() {
        let sep_style = Style::default().fg(Color::DarkGray);
        items.push(ListItem::new("────────────").style(sep_style));
    }
    let offset = if app.tables.is_empty() {
        0
    } else {
        app.tables.len() + 1
    };
    for (i, query) in app.queries.iter().enumerate() {
        let sidebar_idx = offset + i;
        let style = if sidebar_idx == app.selected_sidebar {
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        items.push(ListItem::new(query.name.as_str()).style(style));
    }

    let mut sidebar_block = Block::default().title("Views").borders(Borders::ALL);
    if !app.table_focused {
        sidebar_block = sidebar_block.border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    }
    let sidebar_list = List::new(items).block(sidebar_block);
    frame.render_widget(sidebar_list, main_layout[0]);

    // Right column
    if app.is_query_view {
        draw_query_view(frame, main_layout[1], app);
    } else if !app.headers.is_empty() {
        draw_table_view(frame, main_layout[1], app);
    } else {
        let paragraph = Paragraph::new("No table selected or table is empty")
            .block(Block::default().title("Data").borders(Borders::ALL));
        frame.render_widget(paragraph, main_layout[1]);
    }

    // Keybind reference bar
    let keybinds = build_keybinds(app);
    let keybind_line = Line::from(keybinds);
    let keybind_bar = Paragraph::new(keybind_line);

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
        draw_help_modal(frame);
    }

    // Modal overlay
    if app.modal_open {
        draw_modal(frame, app);
    }
}

fn draw_query_view(frame: &mut Frame, area: Rect, app: &mut App) {
    let title = if app.queries.is_empty() {
        "Query".to_string()
    } else {
        let q_idx = app.query_index();
        format!("Query: {}", app.queries[q_idx].name)
    };
    let mut block = Block::default().title(title).borders(Borders::ALL);
    if app.table_focused {
        block = block.border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    }
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let active_filter_count = app.filters.iter().filter(|f| f.is_some()).count() as u16;
    let filter_bar_height = active_filter_count;
    let type_select_height = if app.filter_mode == FilterMode::TypeSelect {
        1
    } else {
        0
    };
    let value_input_height = if app.filter_mode == FilterMode::ValueInput {
        1
    } else {
        0
    };
    let rename_height = if app.rename_mode { 1 } else { 0 };

    let mut constraints: Vec<Constraint> = vec![Constraint::Percentage(40)];
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
    if rename_height > 0 {
        constraints.push(Constraint::Length(rename_height));
    }

    let query_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    let mut layout_idx = 0;

    // Textarea
    let textarea_area = query_layout[layout_idx];
    layout_idx += 1;
    let mut textarea_block = Block::default().title("SQL").borders(Borders::ALL);
    if app.table_focused && app.query_edit_mode {
        textarea_block = textarea_block.border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    }
    let textarea_inner = textarea_block.inner(textarea_area);
    frame.render_widget(textarea_block, textarea_area);

    let textarea_height = textarea_inner.height;
    app.ensure_query_cursor_visible(textarea_height);
    let highlighted = app
        .highlighter
        .highlight("sql", &app.query_text)
        .unwrap_or_else(|_| app.query_text.lines().map(Line::from).collect());
    let display_lines: Vec<Line> = highlighted
        .iter()
        .skip(app.query_scroll)
        .take(textarea_height as usize)
        .cloned()
        .collect();
    let query_paragraph = Paragraph::new(display_lines);
    frame.render_widget(query_paragraph, textarea_inner);

    if app.table_focused && app.query_edit_mode {
        let (line, col) = cursor_line_col(&app.query_text, app.query_cursor);
        if line >= app.query_scroll && (line - app.query_scroll) < textarea_height as usize {
            let cursor_x = textarea_inner.x + col as u16;
            let cursor_y = textarea_inner.y + (line - app.query_scroll) as u16;
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }

    // Render type select dropdown
    if app.filter_mode == FilterMode::TypeSelect {
        let type_select_area = query_layout[layout_idx];
        layout_idx += 1;
        let col_name = &app.headers[app.filter_col];
        render_type_select(frame, type_select_area, col_name, &app.temp_filter_op);
    }

    // Render active filter bar
    if filter_bar_height > 0 {
        let filter_bar_area = query_layout[layout_idx];
        layout_idx += 1;
        render_filter_bar(frame, filter_bar_area, &app.headers, &app.filters);
    }

    // Render value input
    if app.filter_mode == FilterMode::ValueInput {
        let value_input_area = query_layout[layout_idx];
        layout_idx += 1;
        let col_name = &app.headers[app.filter_col];
        render_value_input(
            frame,
            value_input_area,
            col_name,
            &app.temp_filter_op,
            &app.temp_filter_value,
        );
    }

    // Results table
    let table_area = query_layout[layout_idx];
    layout_idx += 1;
    if !app.headers.is_empty() {
        let highlight = app.table_focused && !app.query_edit_mode;
        let (needs_h_scroll, page_size) = render_table_with_block(
            frame,
            table_area,
            "Results",
            &app.headers,
            &app.rows,
            app.h_scroll,
            app.scroll_offset,
            &mut app.table_state,
            highlight,
            app.filter_mode.clone(),
            app.filter_col,
            app.sort_col,
            app.sort_asc,
        );
        app.needs_h_scroll = needs_h_scroll;
        app.page_size = page_size;
    } else {
        let paragraph = Paragraph::new("Run query to see results")
            .block(Block::default().title("Results").borders(Borders::ALL));
        frame.render_widget(paragraph, table_area);
    }

    // Rename input
    if app.rename_mode {
        let rename_area = query_layout[layout_idx];
        let prefix = "Rename: ";
        let rename_line = Line::from(vec![
            Span::raw(prefix),
            Span::styled(&app.rename_value, Style::default().fg(Color::White)),
        ]);
        let rename_paragraph = Paragraph::new(rename_line);
        frame.render_widget(rename_paragraph, rename_area);

        let cursor_x =
            rename_area.x + prefix.chars().count() as u16 + app.rename_value.chars().count() as u16;
        let cursor_y = rename_area.y;
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}

fn draw_table_view(frame: &mut Frame, area: Rect, app: &mut App) {
    let active_filter_count = app.filters.iter().filter(|f| f.is_some()).count() as u16;
    let filter_bar_height = active_filter_count;
    let type_select_height = if app.filter_mode == FilterMode::TypeSelect {
        1
    } else {
        0
    };
    let value_input_height = if app.filter_mode == FilterMode::ValueInput {
        1
    } else {
        0
    };

    let col_widths = compute_col_widths(&app.headers, &app.rows);
    let inner_width = area.width.saturating_sub(2);
    let spacing = 1;
    let total_table_width = col_widths.iter().copied().sum::<u16>()
        + spacing * (app.headers.len().saturating_sub(1) as u16);
    app.needs_h_scroll = total_table_width > inner_width;

    let (_, end_col) = visible_column_range(app.h_scroll, &app.headers, &col_widths, inner_width);

    let title = if app.headers.len() > (end_col - app.h_scroll) {
        format!(
            "Table: {} (cols {}-{} of {})",
            app.tables[app.selected_sidebar],
            app.h_scroll + 1,
            end_col,
            app.headers.len()
        )
    } else {
        format!("Table: {}", app.tables[app.selected_sidebar])
    };

    let mut block = Block::default().title(title).borders(Borders::ALL);
    if app.table_focused {
        block = block.border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    }
    let right_inner = block.inner(area);
    frame.render_widget(block, area);

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

    if app.filter_mode == FilterMode::TypeSelect {
        let type_select_area = right_layout[layout_idx];
        layout_idx += 1;
        let col_name = &app.headers[app.filter_col];
        render_type_select(frame, type_select_area, col_name, &app.temp_filter_op);
    }

    if filter_bar_height > 0 {
        let filter_bar_area = right_layout[layout_idx];
        layout_idx += 1;
        render_filter_bar(frame, filter_bar_area, &app.headers, &app.filters);
    }

    if app.filter_mode == FilterMode::ValueInput {
        let value_input_area = right_layout[layout_idx];
        layout_idx += 1;
        let col_name = &app.headers[app.filter_col];
        render_value_input(
            frame,
            value_input_area,
            col_name,
            &app.temp_filter_op,
            &app.temp_filter_value,
        );
    }

    let table_area = right_layout[layout_idx];
    let page_size = render_table_widget(
        frame,
        table_area,
        &app.headers,
        &app.rows,
        app.h_scroll,
        app.scroll_offset,
        &mut app.table_state,
        app.table_focused,
        app.filter_mode.clone(),
        app.filter_col,
        app.sort_col,
        app.sort_asc,
        &col_widths,
        end_col,
    );
    app.page_size = page_size;
}

fn draw_help_modal(frame: &mut Frame) {
    let area = centered_rect(60, 60, frame.area());
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
        "Sidebar",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )));
    help_lines.push(Line::from("  j / Down   : Next item"));
    help_lines.push(Line::from("  k / Up     : Previous item"));
    help_lines.push(Line::from("  Tab / Enter: View selected"));
    help_lines.push(Line::from("  n          : New query"));
    help_lines.push(Line::from("  D          : Delete query (in Queries)"));
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
    help_lines.push(Line::from("  Tab        : Back to sidebar"));
    help_lines.push(Line::from("  Enter      : Open Details"));
    help_lines.push(Line::from("  /          : Filter mode"));
    help_lines.push(Line::from(""));
    help_lines.push(Line::from(Span::styled(
        "Query View",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )));
    help_lines.push(Line::from("  Tab        : Edit SQL"));
    help_lines.push(Line::from("  Esc        : Back to sidebar"));
    help_lines.push(Line::from("  r          : Rename query"));
    help_lines.push(Line::from("  /          : Filter mode"));
    help_lines.push(Line::from("  j / Down   : Scroll results down"));
    help_lines.push(Line::from("  k / Up     : Scroll results up"));
    help_lines.push(Line::from("  h / Left   : Scroll results left"));
    help_lines.push(Line::from("  l / Right  : Scroll results right"));
    help_lines.push(Line::from("  PgUp/PgDn  : Page results"));
    help_lines.push(Line::from(""));
    help_lines.push(Line::from(Span::styled(
        "Query Edit Mode",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )));
    help_lines.push(Line::from("  Esc / Tab  : Back to results"));
    help_lines.push(Line::from("  Enter      : New line"));
    help_lines.push(Line::from("  Ctrl+Enter : Run query"));
    help_lines.push(Line::from("  Backspace  : Delete previous"));
    help_lines.push(Line::from("  Delete     : Delete next"));
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
        "Details Modal",
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

fn draw_modal(frame: &mut Frame, app: &mut App) {
    let area = centered_rect(80, 80, frame.area());
    frame.render_widget(Clear, area);
    let title = if app.is_query_view {
        "Row Data"
    } else {
        "Foreign Key Records"
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    frame.render_widget(block, area);

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
        let record_count = app.modal_records.len() as u16;
        let mut constraints = Vec::new();
        for _ in 0..record_count {
            constraints.push(Constraint::Length(4));
        }
        constraints.push(Constraint::Fill(1));
        let record_areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        app.modal_needs_h_scroll = false;

        for (i, record) in app.modal_records.iter().enumerate() {
            let record_area = record_areas[i];
            let is_selected = i == app.modal_selected;
            let title = if app.is_query_view {
                format!("Row {} of {}", record.fk_value, app.rows.len())
            } else {
                format!(
                    "{} → {} ({} = {})",
                    record.fk_column, record.table_name, record.ref_column, record.fk_value
                )
            };
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

            let table_area = Rect::new(
                record_area.x + 1,
                record_area.y + 1,
                record_area.width.saturating_sub(2),
                record_area.height.saturating_sub(2),
            );

            let needs_h_scroll = render_modal_table(
                frame,
                table_area,
                &record.headers,
                &record.row,
                app.modal_h_scroll,
            );
            if needs_h_scroll {
                app.modal_needs_h_scroll = true;
            }
        }
    }
}

fn build_keybinds(app: &App) -> Vec<Span<'_>> {
    if app.modal_open {
        vec![
            Span::raw(" q"),
            Span::styled(": Quit   ", Style::default().fg(Color::DarkGray)),
            Span::raw("Esc"),
            Span::styled(": Close   ", Style::default().fg(Color::DarkGray)),
            Span::raw("Enter"),
            Span::styled(": Go to Table", Style::default().fg(Color::DarkGray)),
        ]
    } else if app.table_focused {
        if app.filter_mode != FilterMode::None {
            match app.filter_mode {
                FilterMode::HeaderSelect => vec![
                    Span::raw(" q"),
                    Span::styled(": Quit   ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Esc"),
                    Span::styled(": Cancel   ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Enter"),
                    Span::styled(": Add/Edit   ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Del"),
                    Span::styled(": Remove", Style::default().fg(Color::DarkGray)),
                ],
                FilterMode::TypeSelect => vec![
                    Span::raw(" q"),
                    Span::styled(": Quit   ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Esc"),
                    Span::styled(": Cancel   ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Enter"),
                    Span::styled(": Value   ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Del"),
                    Span::styled(": Remove", Style::default().fg(Color::DarkGray)),
                ],
                FilterMode::ValueInput => vec![
                    Span::raw(" q"),
                    Span::styled(": Quit   ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Esc"),
                    Span::styled(": Cancel   ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Enter"),
                    Span::styled(": Apply   ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Del"),
                    Span::styled(": Remove", Style::default().fg(Color::DarkGray)),
                ],
                FilterMode::None => unreachable!(),
            }
        } else if app.is_query_view {
            if app.rename_mode {
                vec![
                    Span::raw(" q"),
                    Span::styled(": Quit   ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Esc"),
                    Span::styled(": Cancel   ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Enter"),
                    Span::styled(": Rename   ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Bksp"),
                    Span::styled(": Delete", Style::default().fg(Color::DarkGray)),
                ]
            } else if app.query_edit_mode {
                vec![
                    Span::raw(" q"),
                    Span::styled(": Quit   ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Esc/Tab"),
                    Span::styled(": Results   ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Ctrl+Enter"),
                    Span::styled(": Run   ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Bksp"),
                    Span::styled(": Delete", Style::default().fg(Color::DarkGray)),
                ]
            } else {
                vec![
                    Span::raw(" q"),
                    Span::styled(": Quit   ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Esc"),
                    Span::styled(": Views   ", Style::default().fg(Color::DarkGray)),
                    Span::raw("Tab"),
                    Span::styled(": SQL   ", Style::default().fg(Color::DarkGray)),
                    Span::raw("r"),
                    Span::styled(": Rename   ", Style::default().fg(Color::DarkGray)),
                    Span::raw("/"),
                    Span::styled(": Filter", Style::default().fg(Color::DarkGray)),
                ]
            }
        } else {
            vec![
                Span::raw(" q"),
                Span::styled(": Quit   ", Style::default().fg(Color::DarkGray)),
                Span::raw("Tab"),
                Span::styled(": Table List   ", Style::default().fg(Color::DarkGray)),
                Span::raw("Enter"),
                Span::styled(": Details   ", Style::default().fg(Color::DarkGray)),
                Span::raw("/"),
                Span::styled(": Filter", Style::default().fg(Color::DarkGray)),
            ]
        }
    } else {
        let mut binds = vec![
            Span::raw(" q"),
            Span::styled(": Quit   ", Style::default().fg(Color::DarkGray)),
            Span::raw("Tab"),
            Span::styled(": View   ", Style::default().fg(Color::DarkGray)),

        ];
        if app.current_is_query() {
            binds.extend(vec![
                Span::raw(" n"),
                Span::styled(": New   ", Style::default().fg(Color::DarkGray)),
                Span::raw("D"),
                Span::styled(": Del", Style::default().fg(Color::DarkGray)),
            ]);
        }
        binds
    }
}
