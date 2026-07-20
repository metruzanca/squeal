use std::io::{self, Stdout};

use artbox::{Alignment as ArtAlignment, Renderer};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::config::{censor_connection_string, detect_databases, generate_db_name, Config, RecentEntry};

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let x_offset = area.width.saturating_sub(area.width * percent_x / 100) / 2;
    let y_offset = area.height.saturating_sub(area.height * percent_y / 100) / 2;
    Rect {
        x: area.x + x_offset,
        y: area.y + y_offset,
        width: area.width * percent_x / 100,
        height: area.height * percent_y / 100,
    }
}

pub struct StartupScreen {
    pub entries: Vec<RecentEntry>,
    pub detected: Vec<RecentEntry>,
    pub selected: usize,
    pub rename_mode: bool,
    pub rename_input: String,
    pub rename_combined_idx: usize,
    pub new_connection_mode: bool,
    pub new_connection_input: String,
}

impl StartupScreen {
    pub fn new(entries: Vec<RecentEntry>, detected: Vec<RecentEntry>) -> Self {
        Self {
            selected: 0,
            entries,
            detected,
            rename_mode: false,
            rename_input: String::new(),
            rename_combined_idx: 0,
            new_connection_mode: false,
            new_connection_input: String::new(),
        }
    }

    fn total_entries(&self) -> usize {
        self.detected.len() + self.entries.len()
    }

    fn all_entries(&self) -> impl Iterator<Item = &RecentEntry> {
        self.detected.iter().chain(self.entries.iter())
    }

    fn entry_name(entry: &RecentEntry) -> String {
        entry.name.clone().unwrap_or_else(|| generate_db_name(&entry.path))
    }

    pub fn draw(&self, frame: &mut Frame) {
        let area = frame.area();
        frame.render_widget(Clear, area);

        if self.new_connection_mode {
            let modal_area = centered_rect(60, 30, area);
            frame.render_widget(Clear, modal_area);
            let block = Block::default()
                .title(" New Connection ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow));
            frame.render_widget(block, modal_area);

            let inner = Rect {
                x: modal_area.x + 2,
                y: modal_area.y + 1,
                width: modal_area.width.saturating_sub(4),
                height: modal_area.height.saturating_sub(2),
            };

            let v_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(3),
                    Constraint::Fill(1),
                    Constraint::Length(1),
                ])
                .split(inner);

            let prompt = Paragraph::new("Connection string or file path:")
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(prompt, v_layout[0]);

            let input_text = if self.new_connection_input.is_empty() {
                " ".to_string()
            } else {
                self.new_connection_input.clone()
            };
            let input_para = Paragraph::new(Span::styled(
                input_text,
                Style::default().add_modifier(Modifier::UNDERLINED),
            ));
            frame.render_widget(input_para, v_layout[2]);

            let hint = Line::from(vec![
                Span::styled("Enter", Style::default().fg(Color::Yellow)),
                Span::raw(": Connect  "),
                Span::styled("Esc", Style::default().fg(Color::Yellow)),
                Span::raw(": Cancel"),
            ]);
            let hint_para = Paragraph::new(hint).alignment(Alignment::Center);
            frame.render_widget(hint_para, v_layout[4]);

            return;
        }

        let v_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(10),
                Constraint::Length(9),
                Constraint::Length(1),
                Constraint::Min(5),
                Constraint::Length(2),
                Constraint::Length(3),
                Constraint::Percentage(10),
            ])
            .split(area);

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

        let subtitle = Paragraph::new(Span::styled(
            "A lightweight TUI database viewer",
            Style::default().fg(Color::DarkGray),
        ))
        .alignment(Alignment::Center);
        let subtitle_area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Fill(1), Constraint::Length(1)])
            .split(v_layout[1])[1];
        frame.render_widget(subtitle, subtitle_area);

        let content_area = v_layout[3];
        let has_any = self.total_entries() > 0;

        if !has_any {
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

            let all: Vec<&RecentEntry> = self.all_entries().collect();
            let max_badge_len = all
                .iter()
                .map(|e| e.connection_type.len() + 2)
                .max()
                .unwrap_or(0);
            let max_name_len = all
                .iter()
                .map(|e| Self::entry_name(e).chars().count())
                .max()
                .unwrap_or(0);
            let max_path_len = all
                .iter()
                .map(|e| e.path.chars().count())
                .max()
                .unwrap_or(0);
            let content_width =
                (max_badge_len + 2 + max_name_len + 2 + max_path_len)
                    .max("Recent Databases".len()) as u16;

            for (i, entry) in all.iter().enumerate() {
                let is_rename_target =
                    self.rename_mode && i == self.rename_combined_idx;
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
                } else if entry.connection_type == "env" {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Cyan)
                };

                let censored = censor_connection_string(&entry.path);

                let spans = if is_rename_target {
                    let input_display = if self.rename_input.is_empty() {
                        " ".to_string()
                    } else {
                        self.rename_input.clone()
                    };
                    vec![
                        Span::styled(badge_padded, badge_style),
                        Span::raw("  "),
                        Span::styled(">", Style::default().fg(Color::Yellow)),
                        Span::styled(
                            format!("{:<width$}", input_display, width = max_name_len - 1),
                            Style::default().add_modifier(Modifier::UNDERLINED),
                        ),
                        Span::raw("  "),
                        Span::styled(censored, Style::default().fg(Color::DarkGray)),
                    ]
                } else {
                    let name = Self::entry_name(entry);
                    vec![
                        Span::styled(badge_padded, badge_style),
                        Span::raw("  "),
                        Span::styled(
                            format!("{:<width$}", name, width = max_name_len),
                            style,
                        ),
                        Span::raw("  "),
                        Span::styled(censored, Style::default().fg(Color::DarkGray)),
                    ]
                };
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

        let footer_spans = if self.rename_mode {
            vec![
                Span::styled("Enter", Style::default().fg(Color::Yellow)),
                Span::raw(": confirm rename  "),
                Span::styled("Esc", Style::default().fg(Color::Yellow)),
                Span::raw(": cancel"),
            ]
        } else if !has_any {
            vec![
                Span::styled("N", Style::default().fg(Color::Yellow)),
                Span::raw(": New  "),
                Span::styled("q", Style::default().fg(Color::Yellow)),
                Span::raw(": quit"),
            ]
        } else {
            vec![
                Span::styled("j/k/\u{2191}/\u{2193}", Style::default().fg(Color::Yellow)),
                Span::raw(": navigate  "),
                Span::styled("N", Style::default().fg(Color::Yellow)),
                Span::raw(": New  "),
                Span::styled("R", Style::default().fg(Color::Yellow)),
                Span::raw(": rename  "),
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

        let credit_lines = vec![
            Line::from(vec![
                Span::styled("Made with ", Style::default().fg(Color::DarkGray)),
                Span::styled("\u{2665}", Style::default().fg(Color::Red)),
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
    let detected = detect_databases();
    let mut screen = StartupScreen::new(config.recent.clone(), detected);

    loop {
        terminal.draw(|frame| screen.draw(frame))?;

        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            if screen.new_connection_mode {
                match key.code {
                    KeyCode::Enter => {
                        let input = screen.new_connection_input.trim().to_string();
                        if !input.is_empty() {
                            let conn_type = if input.starts_with("postgres://")
                                || input.starts_with("postgresql://")
                            {
                                "postgres"
                            } else {
                                "sqlite"
                            };
                            config.add_recent(&input, conn_type);
                            let _ = config.save();
                            return Ok(Some(input));
                        }
                    }
                    KeyCode::Esc => {
                        screen.new_connection_mode = false;
                        screen.new_connection_input.clear();
                    }
                    KeyCode::Backspace => {
                        screen.new_connection_input.pop();
                    }
                    KeyCode::Delete => {
                        screen.new_connection_input.clear();
                    }
                    KeyCode::Char(c) => {
                        screen.new_connection_input.push(c);
                    }
                    _ => {}
                }
            } else if screen.rename_mode {
                match key.code {
                    KeyCode::Enter => {
                        let is_detected =
                            screen.rename_combined_idx < screen.detected.len();
                        if is_detected {
                            let idx = screen.rename_combined_idx;
                            if let Some(entry) = screen.detected.get_mut(idx) {
                                entry.name = Some(screen.rename_input.clone());
                            }
                        } else {
                            let idx = screen.rename_combined_idx - screen.detected.len();
                            config.rename_recent(idx, &screen.rename_input);
                            let _ = config.save();
                            screen.entries = config.recent.clone();
                        }
                        screen.rename_mode = false;
                    }
                    KeyCode::Esc => {
                        screen.rename_mode = false;
                    }
                    KeyCode::Backspace => {
                        screen.rename_input.pop();
                    }
                    KeyCode::Delete => {
                        screen.rename_input.clear();
                    }
                    KeyCode::Char(c) => {
                        screen.rename_input.push(c);
                    }
                    _ => {}
                }
            } else {
                match key.code {
                    KeyCode::Char('q') => return Ok(None),
                    KeyCode::Char('j') | KeyCode::Down => {
                        let total = screen.total_entries();
                        if total > 0 {
                            screen.selected = (screen.selected + 1) % total;
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        let total = screen.total_entries();
                        if total > 0 {
                            screen.selected = if screen.selected == 0 {
                                total - 1
                            } else {
                                screen.selected - 1
                            };
                        }
                    }
                    KeyCode::Char('N') => {
                        screen.new_connection_mode = true;
                        screen.new_connection_input.clear();
                    }
                    KeyCode::Char('R') => {
                        let total = screen.total_entries();
                        if total > 0 {
                            let entry = if screen.selected < screen.detected.len() {
                                &screen.detected[screen.selected]
                            } else {
                                &screen.entries[screen.selected - screen.detected.len()]
                            };
                            screen.rename_mode = true;
                            screen.rename_combined_idx = screen.selected;
                            screen.rename_input =
                                StartupScreen::entry_name(entry).to_string();
                        }
                    }
                    KeyCode::Enter => {
                        let total = screen.total_entries();
                        if total > 0 {
                            let path = if screen.selected < screen.detected.len() {
                                screen.detected[screen.selected].path.clone()
                            } else {
                                screen.entries[screen.selected - screen.detected.len()]
                                    .path
                                    .clone()
                            };
                            config.add_recent(
                                &path,
                                if path.starts_with("postgres://")
                                    || path.starts_with("postgresql://")
                                {
                                    "postgres"
                                } else {
                                    "sqlite"
                                },
                            );
                            let _ = config.save();
                            return Ok(Some(path));
                        }
                    }
                    KeyCode::Delete | KeyCode::Backspace => {
                        if screen.selected >= screen.detected.len() {
                            let recent_idx = screen.selected - screen.detected.len();
                            config.remove_recent(recent_idx);
                            let _ = config.save();
                            screen.entries = config.recent.clone();
                            let total = screen.total_entries();
                            if screen.selected >= total && total > 0 {
                                screen.selected = total - 1;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
