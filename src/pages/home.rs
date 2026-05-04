use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::credential::CredentialStore;

#[derive(Clone, Copy, PartialEq)]
pub enum HomeFocus {
    SearchBox,
    Playlists,
    Credential,
    CleanFiles,
    Settings,
}

pub enum SearchType {
    Song,
    Album,
}

pub struct HomeState {
    pub focus: HomeFocus,
    pub providers: Vec<String>,
    pub current_provider_index: usize,
    pub message: String,
    pub show_clean_confirm: bool,
    pub clean_confirm_yes: bool,
    pub search_text: String,
    pub cursor_pos: usize,
    pub search_type: SearchType,
    pub sel_start: Option<usize>,
    pub sel_end: Option<usize>,
}

impl HomeState {
    pub fn new(provider_names: Vec<String>) -> Self {
        let providers = provider_names.clone();
        Self {
            focus: if provider_names.is_empty() { HomeFocus::CleanFiles } else { HomeFocus::SearchBox },
            providers,
            current_provider_index: 0,
            message: String::new(),
            show_clean_confirm: false,
            clean_confirm_yes: false,
            search_text: String::new(),
            cursor_pos: 0,
            search_type: SearchType::Song,
            sel_start: None,
            sel_end: None,
        }
    }

    pub fn current_provider_name(&self) -> &str {
        if self.providers.is_empty() {
            "无"
        } else {
            &self.providers[self
                .current_provider_index
                .min(self.providers.len().saturating_sub(1))]
        }
    }

    pub fn cycle_provider(&mut self) {
        if !self.providers.is_empty() {
            self.current_provider_index =
                (self.current_provider_index + 1) % self.providers.len();
        }
    }

    fn char_to_byte(&self, char_pos: usize) -> usize {
        self.search_text
            .char_indices()
            .nth(char_pos)
            .map(|(i, _)| i)
            .unwrap_or(self.search_text.len())
    }

    pub fn insert_char(&mut self, c: char) {
        self.delete_selection();
        let byte_pos = self.char_to_byte(self.cursor_pos);
        self.search_text.insert(byte_pos, c);
        self.cursor_pos += 1;
        self.clear_selection();
    }

    pub fn delete_char(&mut self) {
        if self.has_selection() {
            self.delete_selection();
            self.clear_selection();
        } else if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            let byte_pos = self.char_to_byte(self.cursor_pos);
            let end = byte_pos
                + self.search_text[byte_pos..]
                    .chars()
                    .next()
                    .map_or(1, |c| c.len_utf8());
            self.search_text.drain(byte_pos..end);
        }
    }

    pub fn cursor_left(&mut self, shift: bool) {
        if shift {
            if self.sel_start.is_none() {
                self.sel_start = Some(self.cursor_pos);
            }
            self.cursor_pos = self.cursor_pos.saturating_sub(1);
            self.sel_end = Some(self.cursor_pos);
        } else {
            self.cursor_pos = self.cursor_pos.saturating_sub(1);
            self.clear_selection();
        }
    }

    pub fn cursor_right(&mut self, shift: bool) {
        if shift {
            if self.sel_start.is_none() {
                self.sel_start = Some(self.cursor_pos);
            }
            self.cursor_pos = (self.cursor_pos + 1).min(self.search_text.chars().count());
            self.sel_end = Some(self.cursor_pos);
        } else {
            self.cursor_pos = (self.cursor_pos + 1).min(self.search_text.chars().count());
            self.clear_selection();
        }
    }

    pub fn home_key(&mut self, shift: bool) {
        if shift {
            if self.sel_start.is_none() {
                self.sel_start = Some(self.cursor_pos);
            }
            self.cursor_pos = 0;
            self.sel_end = Some(0);
        } else {
            self.cursor_pos = 0;
            self.clear_selection();
        }
    }

    pub fn end_key(&mut self, shift: bool) {
        let end = self.search_text.chars().count();
        if shift {
            if self.sel_start.is_none() {
                self.sel_start = Some(self.cursor_pos);
            }
            self.cursor_pos = end;
            self.sel_end = Some(end);
        } else {
            self.cursor_pos = end;
            self.clear_selection();
        }
    }

    pub fn select_all(&mut self) {
        let len = self.search_text.chars().count();
        if len > 0 {
            self.sel_start = Some(0);
            self.sel_end = Some(len);
            self.cursor_pos = len;
        }
    }

    fn has_selection(&self) -> bool {
        match (self.sel_start, self.sel_end) {
            (Some(s), Some(e)) => s != e,
            _ => false,
        }
    }

    fn selection_range(&self) -> Option<(usize, usize)> {
        match (self.sel_start, self.sel_end) {
            (Some(s), Some(e)) if s != e => {
                let lo = s.min(e);
                let hi = s.max(e);
                Some((lo, hi))
            }
            _ => None,
        }
    }

    fn delete_selection(&mut self) {
        if let Some((lo, hi)) = self.selection_range() {
            let lo_byte = self.char_to_byte(lo);
            let hi_byte = self.char_to_byte(hi);
            self.search_text.drain(lo_byte..hi_byte);
            self.cursor_pos = lo;
        }
    }

    fn clear_selection(&mut self) {
        self.sel_start = None;
        self.sel_end = None;
    }

    pub fn has_providers(&self) -> bool {
        !self.providers.is_empty()
    }

    pub fn focus_next(&mut self) {
        let no_prov = !self.has_providers();
        self.focus = match self.focus {
            HomeFocus::SearchBox => {
                if no_prov { HomeFocus::CleanFiles } else { HomeFocus::Playlists }
            }
            HomeFocus::Playlists => HomeFocus::Credential,
            HomeFocus::Credential => HomeFocus::CleanFiles,
            HomeFocus::CleanFiles => HomeFocus::Settings,
            HomeFocus::Settings => {
                if no_prov { HomeFocus::CleanFiles } else { HomeFocus::SearchBox }
            }
        };
    }

    pub fn focus_prev(&mut self) {
        let no_prov = !self.has_providers();
        self.focus = match self.focus {
            HomeFocus::SearchBox => HomeFocus::Settings,
            HomeFocus::Playlists => HomeFocus::SearchBox,
            HomeFocus::Credential => HomeFocus::Playlists,
            HomeFocus::CleanFiles => {
                if no_prov { HomeFocus::Settings } else { HomeFocus::Credential }
            }
            HomeFocus::Settings => HomeFocus::CleanFiles,
        };
    }
}

pub fn render_home(frame: &mut Frame, state: &HomeState, _credential: &CredentialStore, provider_display: &str) {
    let area = frame.area();
    if area.width < 10 || area.height < 10 {
        return;
    }

    // The render function takes an additional parameter for display name
    // But we can't pass it through the existing render_home signature.
    // Instead, the current_provider_name() already returns the provider ID.
    // We'll change the layout.

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .margin(2)
        .split(area);

    // Title
    frame.render_widget(
        Paragraph::new("Penguin Downloader TUI")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        chunks[0],
    );

    let provider_text = if state.has_providers() {
        format!("当前音源: {}", provider_display)
    } else {
        "当前音源: 无".to_string()
    };
    let plugin_text = format!("已加载插件: {}", state.providers.len());
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(&provider_text, Style::default().fg(Color::Yellow))),
            Line::from(Span::styled(&plugin_text, Style::default().fg(Color::DarkGray))),
        ])
        .alignment(Alignment::Center),
        chunks[1],
    );

    let mode_text = match state.search_type {
        SearchType::Song => "歌曲",
        SearchType::Album => "专辑",
    };
    let search_focused = state.focus == HomeFocus::SearchBox;
    let has_prov = state.has_providers();

    let mut input_spans = vec![
        Span::styled(
            format!("[{}]", mode_text),
            if has_prov {
                Style::default()
                    .fg(if matches!(state.search_type, SearchType::Song) {
                        Color::Yellow
                    } else {
                        Color::Magenta
                    })
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ),
        Span::raw(" "),
    ];

    if !has_prov {
        input_spans.push(Span::styled("无可用音源", Style::default().fg(Color::DarkGray)));
    } else if state.search_text.is_empty() {
        if search_focused {
            input_spans.push(Span::styled("|", Style::default().fg(Color::Green)));
        }
        input_spans.push(Span::styled("输入搜索关键词...", Style::default().fg(Color::DarkGray)));
    } else if search_focused {
        let sel_range = state.selection_range();
        let text_len = state.search_text.chars().count();
        for i in 0..text_len {
            let byte_i = state.char_to_byte(i);
            let ch = state.search_text[byte_i..].chars().next().unwrap();
            let is_sel = sel_range.map(|(lo, hi)| i >= lo && i < hi).unwrap_or(false);
            if is_sel {
                input_spans.push(Span::styled(
                    ch.to_string(),
                    Style::default().fg(Color::White).bg(Color::Blue).add_modifier(Modifier::BOLD),
                ));
            } else if i == state.cursor_pos {
                input_spans.push(Span::styled(
                    ch.to_string(),
                    Style::default().fg(Color::Green).add_modifier(Modifier::REVERSED),
                ));
            } else {
                input_spans.push(Span::raw(ch.to_string()));
            }
        }
        if state.cursor_pos >= text_len {
            input_spans.push(Span::styled(" ", Style::default().fg(Color::Green).bg(Color::Green)));
        }
    } else {
        input_spans.push(Span::raw(&state.search_text));
    }

    frame.render_widget(
        Paragraph::new(Line::from(input_spans))
            .block(
                Block::default()
                    .title("搜索 (Tab 切换类型, Enter 搜索)")
                    .borders(Borders::ALL)
                .border_style(if !has_prov {
                    Style::default().fg(Color::DarkGray)
                } else if search_focused {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::DarkGray)
                }),
            ),
        chunks[2],
    );

    let items: Vec<(&str, HomeFocus, bool)> = vec![
        ("下载歌单", HomeFocus::Playlists, state.has_providers()),
        ("登录管理", HomeFocus::Credential, state.has_providers()),
        ("清理文件", HomeFocus::CleanFiles, true),
        ("设置", HomeFocus::Settings, true),
    ];

    for (i, (label, f, enabled)) in items.iter().enumerate() {
        let sel = state.focus == *f;
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("{}{}", if sel && *enabled { "> " } else { "  " }, label),
                if sel && *enabled {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else if !*enabled {
                    Style::default().fg(Color::DarkGray)
                } else {
                    Style::default().fg(Color::White)
                },
            )))
            .alignment(Alignment::Center),
            chunks[3 + i],
        );
    }

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("↑↓", Style::default().fg(Color::Yellow)),
                Span::raw(" 移动  "),
                Span::styled("Tab", Style::default().fg(Color::Yellow)),
                Span::raw(" 切换搜索类型  "),
                Span::styled("Ctrl+P", Style::default().fg(Color::Yellow)),
                Span::raw(" 切换音源  "),
                Span::styled("Enter", Style::default().fg(Color::Yellow)),
                Span::raw(" 搜索/确认  "),
                Span::styled("Esc", Style::default().fg(Color::Yellow)),
                Span::raw(" 退出"),
            ]),
            Line::from(state.message.clone()),
        ])
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: true }),
        chunks[8],
    );

    if state.show_clean_confirm {
        render_clean_confirm(frame, area, state.clean_confirm_yes);
    }
}

fn render_clean_confirm(frame: &mut Frame, area: Rect, yes: bool) {
    let w = (area.width * 50 / 100).min(46).max(30);
    let h = 7u16;
    let x = area.x + (area.width - w) / 2;
    let y = area.y + (area.height - h) / 2;
    let d = Rect::new(x, y, w, h);
    frame.render_widget(Clear, d);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![Span::styled("确认清理所有下载文件?", Style::default().fg(Color::Yellow))]),
            Line::from(""),
            Line::from(vec![
                Span::styled(if yes { " > 是 " } else { "   是 " }, Style::default().fg(if yes { Color::Green } else { Color::Gray })),
                Span::raw("  "),
                Span::styled(if !yes { " > 否 " } else { "   否 " }, Style::default().fg(if !yes { Color::Red } else { Color::Gray })),
            ]),
            Line::from(""),
        ])
        .block(Block::default().borders(Borders::ALL).title("清理确认"))
        .alignment(Alignment::Center),
        d,
    );
}

fn centered_rect(px: u16, py: u16, r: Rect) -> Rect {
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - py) / 2),
            Constraint::Percentage(py),
            Constraint::Percentage((100 - py) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - px) / 2),
            Constraint::Percentage(px),
            Constraint::Percentage((100 - px) / 2),
        ])
        .split(v[1])[1]
}
