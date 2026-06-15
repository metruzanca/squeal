use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::driver::{ColumnType, FilterOp};

fn op_symbol(op: &FilterOp) -> &'static str {
    match op {
        FilterOp::Equals => "=",
        FilterOp::NotEquals => "≠",
        FilterOp::Contains => "~",
        FilterOp::GreaterThan => ">",
        FilterOp::LessThan => "<",
        FilterOp::GreaterThanOrEquals => "≥",
        FilterOp::LessThanOrEquals => "≤",
    }
}

fn ops_for_type(col_type: &ColumnType) -> Vec<FilterOp> {
    match col_type {
        ColumnType::Number => vec![
            FilterOp::Equals,
            FilterOp::NotEquals,
            FilterOp::Contains,
            FilterOp::GreaterThan,
            FilterOp::LessThan,
            FilterOp::GreaterThanOrEquals,
            FilterOp::LessThanOrEquals,
        ],
        _ => vec![
            FilterOp::Equals,
            FilterOp::NotEquals,
            FilterOp::Contains,
        ],
    }
}

pub fn render_type_select(
    frame: &mut Frame,
    area: Rect,
    col_name: &str,
    temp_filter_op: &FilterOp,
    col_type: &ColumnType,
) {
    let ops = ops_for_type(col_type);
    let mut spans: Vec<Span> = vec![Span::raw(format!("Filter: {} | ", col_name))];
    for (i, op) in ops.iter().enumerate() {
        let style = if *temp_filter_op == *op {
            Style::default().fg(Color::White).add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        spans.push(Span::styled(op_symbol(op), style));
        if i + 1 < ops.len() {
            spans.push(Span::raw("  "));
        }
    }
    let type_line = Line::from(spans);
    let type_paragraph = Paragraph::new(type_line);
    frame.render_widget(type_paragraph, area);
}

pub fn render_filter_bar(
    frame: &mut Frame,
    area: Rect,
    headers: &[String],
    filters: &[Option<(FilterOp, String)>],
) {
    let filter_lines: Vec<Line> = filters
        .iter()
        .enumerate()
        .filter_map(|(i, f)| {
            f.as_ref().map(|(op, val)| {
                let op_str = op_symbol(op);
                let content = format!("{}: {} {}", headers[i], op_str, val);
                Line::from(Span::styled(content, Style::default().fg(Color::DarkGray)))
            })
        })
        .collect();
    let filter_paragraph = Paragraph::new(filter_lines);
    frame.render_widget(filter_paragraph, area);
}

pub fn render_value_input(
    frame: &mut Frame,
    area: Rect,
    col_name: &str,
    temp_filter_op: &FilterOp,
    temp_filter_value: &str,
) {
    let op_str = op_symbol(temp_filter_op);
    let prefix = format!("Filter: {} {} ", col_name, op_str);
    let value_line = Line::from(vec![
        Span::raw(&prefix),
        Span::styled(
            temp_filter_value,
            Style::default().fg(Color::White),
        ),
    ]);
    let value_paragraph = Paragraph::new(value_line);
    frame.render_widget(value_paragraph, area);

    let cursor_x = area.x
        + prefix.chars().count() as u16
        + temp_filter_value.chars().count() as u16;
    let cursor_y = area.y;
    frame.set_cursor_position((cursor_x, cursor_y));
}
