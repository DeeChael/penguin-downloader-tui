use std::sync::Arc;

use crossterm::event::KeyCode;
use penguin_downloader::{
    model::UserPlaylist,
    provider::MusicProvider,
};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame,
};

pub enum PlaylistAction {
    Back,
    Download(Vec<UserPlaylist>, DownloadConfig),
}

pub struct DownloadConfig {
    pub quality: i32,
}

pub struct PlaylistPageState {
    pub provider_name: String,
    pub playlists: Vec<UserPlaylist>,
    pub selected_indices: Vec<usize>,
    pub current_index: usize,
    pub page: i32,
    pub items_per_page: usize,
    pub loading: bool,
    pub message: String,
    pub quality_index: usize,
    pub quality_options: Vec<(String, i32)>,
}

impl PlaylistPageState {
    pub fn new(provider_name: String, provider: &dyn MusicProvider) -> Self {
        let mut quality_options = vec![("最低".to_string(), -2)];
        let mut provider_q: Vec<_> = provider.quality_levels().iter().collect();
        provider_q.sort_by(|a, b| a.0.cmp(b.0));
        for (level, name) in provider_q {
            quality_options.push((name.clone(), *level));
        }
        quality_options.push(("最高".to_string(), -1));

        let default_q = quality_options.len() - 1;

        Self {
            provider_name,
            playlists: Vec::new(),
            selected_indices: Vec::new(),
            current_index: 0,
            page: 1,
            items_per_page: 20,
            loading: true,
            message: "加载中...".to_string(),
            quality_index: default_q,
            quality_options,
        }
    }

    pub fn get_page_items(&self) -> Vec<&UserPlaylist> {
        let start = (self.page - 1) as usize * self.items_per_page;
        let end = (start + self.items_per_page).min(self.playlists.len());
        self.playlists[start..end].iter().collect()
    }

    pub fn get_total_pages(&self) -> i32 {
        ((self.playlists.len() as f64) / (self.items_per_page as f64)).ceil() as i32
    }

    pub fn get_page_start_index(&self) -> usize {
        (self.page - 1) as usize * self.items_per_page
    }

    pub fn get_selected_playlists(&self) -> Vec<UserPlaylist> {
        self.selected_indices
            .iter()
            .filter_map(|&idx| self.playlists.get(idx).cloned())
            .collect()
    }
}

pub fn render_playlist_ui(frame: &mut Frame, state: &PlaylistPageState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(7),
        ])
        .split(frame.area());

    let title = format!("我的歌单 [{}]", state.provider_name);
    let title_widget = Paragraph::new(title)
        .block(Block::default().title("歌单列表").borders(Borders::ALL));
    frame.render_widget(title_widget, chunks[0]);

    if state.loading {
        let loading = Paragraph::new("加载中...")
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(loading, chunks[1]);
    } else if state.playlists.is_empty() {
        let empty = Paragraph::new(state.message.clone())
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(empty, chunks[1]);
    } else {
        let page_items = state.get_page_items();
        let header = Row::new(vec![
            Cell::from("选择").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("歌单名称").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("歌曲数").style(Style::default().add_modifier(Modifier::BOLD)),
        ]);

        let rows: Vec<Row> = page_items
            .iter()
            .enumerate()
            .map(|(i, playlist)| {
                let is_cursor = i == state.current_index;
                let global_idx = state.get_page_start_index() + i;
                let is_selected = state.selected_indices.contains(&global_idx);
                let check_mark = if is_selected { "[✓]" } else { "[ ]" };

                let style = if is_cursor {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else if is_selected {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                };

                Row::new(vec![
                    Cell::from(check_mark),
                    Cell::from(truncate(&playlist.title, 50)),
                    Cell::from(playlist.song_count.to_string()),
                ])
                .style(style)
            })
            .collect();

        let table = Table::new(
            rows,
            [
                Constraint::Length(6),
                Constraint::Percentage(80),
                Constraint::Length(10),
            ],
        )
        .header(header)
        .block(
            Block::default()
                .title(format!("歌单列表 (第 {}/{} 页)", state.page, state.get_total_pages()))
                .borders(Borders::ALL),
        );
        frame.render_widget(table, chunks[1]);
    }

    let selected_count = state.selected_indices.len();

    let mut status_lines = vec![
        Line::from(vec![
            Span::styled(
                if selected_count > 0 { format!("已选择 {} 个", selected_count) } else { "未选择".to_string() },
                if selected_count > 0 { Style::default().fg(Color::Green).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::Gray) },
            ),
        ]),
        Line::from({
            let mut spans = vec![Span::raw("音质: ")];
            for (i, (name, _)) in state.quality_options.iter().enumerate() {
                if i > 0 { spans.push(Span::raw(" │ ")); }
                if i == state.quality_index {
                    spans.push(Span::styled(format!("[{}]", name), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
                } else { spans.push(Span::styled(name.clone(), Style::default().fg(Color::Gray))); }
            }
            Line::from(spans)
        }),
    ];

    if !state.message.is_empty() {
        status_lines.push(Line::from(Span::styled(&state.message, Style::default().fg(Color::Yellow))));
    }
    let status_widget = Paragraph::new(status_lines).block(Block::default());
    frame.render_widget(status_widget, chunks[2]);

    // Key hints
    let hint = Paragraph::new(Line::from(vec![
        Span::styled("↑↓", Style::default().fg(Color::Yellow)),
        Span::raw(" 移动  "),
        Span::styled("Space", Style::default().fg(Color::Yellow)),
        Span::raw(" 选择  "),
        Span::styled("Enter", Style::default().fg(Color::Yellow)),
        Span::raw(" 下载  "),
        Span::styled("K", Style::default().fg(Color::Yellow)),
        Span::raw(" 音质  "),
        Span::styled("N/M", Style::default().fg(Color::Yellow)),
        Span::raw(" 翻页  "),
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::raw(" 返回"),
    ]))
    .block(Block::default().borders(Borders::ALL).style(Style::default().fg(Color::White)));
    let hint_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(chunks[2])[1];
    frame.render_widget(hint, hint_area);
}

fn truncate(s: &str, max_width: usize) -> String {
    if s.chars().count() > max_width {
        format!("{}...", s.chars().take(max_width.saturating_sub(3)).collect::<String>())
    } else {
        s.to_string()
    }
}

pub async fn handle_playlist_input(
    state: &mut PlaylistPageState,
    key: KeyCode,
    _provider: &Arc<dyn MusicProvider>,
    _credential: Option<&str>,
) -> Option<PlaylistAction> {
    match key {
        KeyCode::Esc => return Some(PlaylistAction::Back),
        KeyCode::Up => {
            let page_items = state.get_page_items();
            if !page_items.is_empty() && state.current_index > 0 {
                state.current_index -= 1;
            }
        }
        KeyCode::Down => {
            let page_items = state.get_page_items();
            if !page_items.is_empty() && state.current_index + 1 < page_items.len() {
                state.current_index += 1;
            }
        }
        KeyCode::Char(' ') => {
            let global_idx = state.get_page_start_index() + state.current_index;
            if let Some(pos) = state.selected_indices.iter().position(|&x| x == global_idx) {
                state.selected_indices.remove(pos);
            } else {
                state.selected_indices.push(global_idx);
            }
        }
        KeyCode::Enter => {
            let selected = state.get_selected_playlists();
            if !selected.is_empty() {
                let (_, quality) = state.quality_options[state.quality_index];
                return Some(PlaylistAction::Download(
                    selected,
                    DownloadConfig {
                        quality,
                    },
                ));
            } else {
                state.message = "请先按空格选择歌单".to_string();
            }
        }
        KeyCode::Char('k') | KeyCode::Char('K') => {
            state.quality_index = (state.quality_index + 1) % state.quality_options.len();
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            if state.page > 1 {
                state.page -= 1;
                state.current_index = 0;
            }
        }
        KeyCode::Char('m') | KeyCode::Char('M') => {
            let total_pages = state.get_total_pages();
            if state.page < total_pages {
                state.page += 1;
                state.current_index = 0;
            }
        }
        _ => {}
    }
    None
}
