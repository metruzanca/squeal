use ratatui::layout::Rect;

pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let width = area.width * percent_x / 100;
    let height = area.height * percent_y / 100;
    let x = area.width.saturating_sub(width) / 2;
    let y = area.height.saturating_sub(height) / 2;
    Rect::new(area.x + x, area.y + y, width, height)
}

pub fn cursor_line_col(text: &str, cursor: usize) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;
    for (i, ch) in text.chars().enumerate() {
        if i >= cursor {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
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
