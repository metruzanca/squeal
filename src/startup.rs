use std::io::{self, Stdout};

use artbox::{Alignment as ArtAlignment, Renderer};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use crate::config::{Config, RecentEntry};

pub struct StartupScreen {
    pub entries: Vec<RecentEntry>,
    pub selected: usize,
}

impl StartupScreen {
    pub fn new(entries: Vec<RecentEntry>) -> Self {
        Self {
            selected: 0,
            entries,
        }
    }

    pub fn draw(&self, frame: &mut Frame) {
        let area = frame.area();
        frame.render_widget(Clear, area);

        let v_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(10), // top spacing
                Constraint::Length(9),      // ASCII art title
                Constraint::Length(1),      // spacer
                Constraint::Min(5),         // content
                Constraint::Length(2),      // footer (keybinds)
                Constraint::Length(3),      // credit lines
                Constraint::Percentage(10), // bottom spacing
            ])
            .split(area);

        // ASCII art title - manually render into buffer due to ratatui version mismatch
        let title_area = v_layout[1];
        let renderer = Renderer::default().with_alignment(ArtAlignment::Center);
        if let Ok(rendered) = renderer.render_grid("Squeal", title_area.width, title_area.height) {
            for (row_idx, row) in rendered.chars.iter().enumerate() {
                let y = title_area.y + row_idx as u16;
                if y >= title_area.y + title_area.height {
                    break;
                }
                for (col_idx, sc) in row.iter().enumerate() {
                    let x = title_area.x + col_idx as u16;
                    if x >= title_area.x + title_area.width {
                        break;
                    }
                    let cell = &mut frame.buffer_mut()[(x, y)];
                    cell.set_char(sc.ch);
                    if let Some(rgb) = sc.fg {
                        cell.set_fg(Color::Rgb(rgb.r, rgb.g, rgb.b));
                    }
                }
            }
        }

        // Subtitle below the art
        let subtitle = Paragraph::new(Span::styled(
            "A lightweight TUI database viewer",
            Style::default().fg(Color::DarkGray),
        ))
        .alignment(Alignment::Center);
        // Place subtitle in a small area just below the art
        let subtitle_area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Fill(1), Constraint::Length(1)])
            .split(v_layout[1])[1];
        frame.render_widget(subtitle, subtitle_area);

        // Content area
        let content_area = v_layout[3];
        if self.entries.is_empty() {
            let lines = vec![
                Line::from("No recent databases."),
                Line::from(""),
                Line::from("Get started with one of these:"),
                Line::from(""),
                Line::from(vec![
                    Span::styled("squeal", Style::default().fg(Color::Yellow)),
                    Span::raw(" my.db"),
                ]),
                Line::from(vec![
                    Span::styled("squeal", Style::default().fg(Color::Yellow)),
                    Span::raw(" postgres://user:pass@host/db"),
                ]),
                Line::from(vec![
                    Span::styled("squeal", Style::default().fg(Color::Yellow)),
                    Span::raw(" --demo"),
                ]),
            ];
            let content_width = lines
                .iter()
                .map(|l| l.spans.iter().map(|s| s.content.len()).sum::<usize>())
                .max()
                .unwrap_or(0) as u16;
            let h_layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Fill(1),
                    Constraint::Length(content_width),
                    Constraint::Fill(1),
                ])
                .split(content_area);
            let msg = Paragraph::new(lines)
                .alignment(Alignment::Left)
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(msg, h_layout[1]);
        } else {
            let mut lines: Vec<Line> = Vec::new();
            lines.push(Line::from(Span::styled(
                "Recent Databases",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));

            // Calculate max width for badge so paths align
            let max_badge_len = self
                .entries
                .iter()
                .map(|e| e.connection_type.len() + 2) // +2 for []
                .max()
                .unwrap_or(0);
            let max_path_len = self
                .entries
                .iter()
                .map(|e| e.path.chars().count())
                .max()
                .unwrap_or(0);
            let content_width =
                (max_badge_len + 2 + max_path_len).max("Recent Databases".len()) as u16;

            for (i, entry) in self.entries.iter().enumerate() {
                let is_selected = i == self.selected;
                let badge = format!("[{}]", entry.connection_type);
                let badge_padded = format!("{:width$}", badge, width = max_badge_len);

                let style = if is_selected {
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let badge_style = if is_selected {
                    style
                } else {
                    Style::default().fg(Color::Cyan)
                };

                let spans = vec![
                    Span::styled(badge_padded, badge_style),
                    Span::raw("  "),
                    Span::styled(&entry.path, style),
                ];
                lines.push(Line::from(spans));
            }

            let h_layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Fill(1),
                    Constraint::Length(content_width),
                    Constraint::Fill(1),
                ])
                .split(content_area);
            let list = Paragraph::new(lines).alignment(Alignment::Left);
            frame.render_widget(list, h_layout[1]);
        }

        // Footer
        let footer_spans = if self.entries.is_empty() {
            vec![
                Span::styled("q", Style::default().fg(Color::Yellow)),
                Span::raw(": quit"),
            ]
        } else {
            vec![
                Span::styled("j/k/\u{2191}/\u{2193}", Style::default().fg(Color::Yellow)),
                Span::raw(": navigate  "),
                Span::styled("Enter", Style::default().fg(Color::Yellow)),
                Span::raw(": open  "),
                Span::styled("Del", Style::default().fg(Color::Yellow)),
                Span::raw(": remove  "),
                Span::styled("q", Style::default().fg(Color::Yellow)),
                Span::raw(": quit"),
            ]
        };
        let footer = Paragraph::new(Line::from(footer_spans)).alignment(Alignment::Center);
        frame.render_widget(footer, v_layout[4]);

        // Credit line
        let credit_lines = vec![
            Line::from(vec![
                Span::styled("Made with ", Style::default().fg(Color::DarkGray)),
                Span::styled("\u{2665}", Style::default().fg(Color::Red)), // ♥
            ]),
            Line::from(Span::styled(
                "https://x.com/metruzanca",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "https://github.com/metruzanca/squeal",
                Style::default().fg(Color::DarkGray),
            )),
        ];
        let credit = Paragraph::new(credit_lines).alignment(Alignment::Center);
        frame.render_widget(credit, v_layout[5]);
    }
}

pub fn run_startup(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> io::Result<Option<String>> {
    let mut config = Config::load().unwrap_or_default();
    let mut screen = StartupScreen::new(config.recent.clone());

    loop {
        terminal.draw(|frame| screen.draw(frame))?;

        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Char('q') => return Ok(None),
                KeyCode::Char('j') | KeyCode::Down => {
                    if !screen.entries.is_empty() {
                        screen.selected = (screen.selected + 1) % screen.entries.len();
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    if !screen.entries.is_empty() {
                        screen.selected = if screen.selected == 0 {
                            screen.entries.len() - 1
                        } else {
                            screen.selected - 1
                        };
                    }
                }
                KeyCode::Enter => {
                    if let Some(entry) = screen.entries.get(screen.selected) {
                        let path = entry.path.clone();
                        config.add_recent(&path, &entry.connection_type);
                        let _ = config.save();
                        return Ok(Some(path));
                    }
                }
                KeyCode::Delete | KeyCode::Backspace => {
                    if !screen.entries.is_empty() {
                        config.remove_recent(screen.selected);
                        let _ = config.save();
                        screen.entries = config.recent.clone();
                        if screen.selected >= screen.entries.len() && !screen.entries.is_empty() {
                            screen.selected = screen.entries.len() - 1;
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
