use std::sync::Arc;

use crossterm::event::KeyCode;
use penguin_downloader::{
    model::{AlbumInfo, AlbumSearchResult},
    provider::{MusicProvider, Pagination},
};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame,
};

pub enum AlbumSearchAction {
    Back,
    Download(Vec<AlbumInfo>, DownloadConfig),
}

pub struct DownloadConfig {
    pub quality: i32,
}

pub struct AlbumSearchState {
    pub keyword: String,
    pub provider_name: String,
    pub albums: Vec<AlbumInfo>,
    pub selected_indices: Vec<usize>,
    pub current_index: usize,
    pub loading: bool,
    pub message: String,
    pub quality_index: usize,
    pub quality_options: Vec<(String, i32)>,
}

impl AlbumSearchState {
    pub fn new(provider_name: String, provider: &dyn MusicProvider) -> Self {
        let mut quality_options = vec![("最低".to_string(), -2)];
        let mut provider_q: Vec<_> = provider.quality_levels().iter().collect();
        provider_q.sort_by(|a, b| a.0.cmp(b.0));
        for (level, name) in provider_q {
            quality_options.push((name.clone(), *level));
        }
        quality_options.push(("最高".to_string(), -1));

        Self {
            keyword: String::new(),
            provider_name,
            albums: Vec::new(),
            selected_indices: Vec::new(),
            current_index: 0,
            loading: true,
            message: "搜索中...".to_string(),
            quality_index: quality_options.len() - 1,
            quality_options,
        }
    }

    pub fn get_selected_albums(&self) -> Vec<AlbumInfo> {
        self.selected_indices
            .iter()
            .filter_map(|&idx| self.albums.get(idx).cloned())
            .collect()
    }
}

pub fn render_album_search_ui(frame: &mut Frame, state: &AlbumSearchState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(7),
        ])
        .split(frame.area());

    let title = format!(
        "搜索专辑: {} [{}]  (Esc 返回)",
        state.keyword, state.provider_name
    );
    let title_widget = Paragraph::new(title)
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(title_widget, chunks[0]);

    if state.loading {
        let loading = Paragraph::new("搜索中...")
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(loading, chunks[1]);
    } else if state.albums.is_empty() {
        let empty = Paragraph::new(state.message.clone())
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(empty, chunks[1]);
    } else {
        let header = Row::new(vec![
            Cell::from("选择").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("专辑名称").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("艺术家").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("歌曲数").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("发行时间").style(Style::default().add_modifier(Modifier::BOLD)),
        ]);

        let rows: Vec<Row> = state
            .albums
            .iter()
            .enumerate()
            .map(|(i, album)| {
                let is_cursor = i == state.current_index;
                let is_selected = state.selected_indices.contains(&i);
                let check_mark = if is_selected { "[✓]" } else { "[ ]" };
                let artist = if album.artists.is_empty() { "未知".to_string() } else { album.artists.join(" / ") };
                let song_count = album
                    .song_count
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "-".to_string());
                let publish_time = album.publish_time.as_deref().unwrap_or("-");

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
                    Cell::from(truncate(&album.title, 30)),
                    Cell::from(truncate(&artist, 20)),
                    Cell::from(song_count),
                    Cell::from(publish_time),
                ])
                .style(style)
            })
            .collect();

        let table = Table::new(
            rows,
            [
                Constraint::Length(6),
                Constraint::Percentage(40),
                Constraint::Percentage(30),
                Constraint::Length(10),
                Constraint::Length(15),
            ],
        )
        .header(header)
        .block(Block::default().title("专辑列表").borders(Borders::ALL));
        frame.render_widget(table, chunks[1]);
    }

    let selected_count = state.selected_indices.len();

    let mut status_lines = vec![
        Line::from(vec![Span::styled(
            if selected_count > 0 { format!("已选择 {} 个", selected_count) } else { "未选择".to_string() },
            if selected_count > 0 { Style::default().fg(Color::Green).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::Gray) },
        )]),
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

    let hint = Paragraph::new(Line::from(vec![
        Span::styled("↑↓", Style::default().fg(Color::Yellow)),
        Span::raw(" 移动  "),
        Span::styled("Space", Style::default().fg(Color::Yellow)),
        Span::raw(" 选择  "),
        Span::styled("Enter", Style::default().fg(Color::Yellow)),
        Span::raw(" 下载  "),
        Span::styled("K", Style::default().fg(Color::Yellow)),
        Span::raw(" 音质  "),
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

pub async fn do_album_search(
    _provider: &Arc<dyn MusicProvider>,
    _keyword: &str,
    _page: i32,
    _credential: Option<&str>,
) -> anyhow::Result<Option<AlbumSearchResult>> {
    match _provider
        .search_albums(_keyword, Pagination::new(10, _page), _credential)
        .await
    {
        Ok(result) => Ok(Some(result)),
        Err(e) => Err(e.into()),
    }
}

pub async fn handle_album_search_input(
    state: &mut AlbumSearchState,
    key: KeyCode,
    _provider: &Arc<dyn MusicProvider>,
    _credential: Option<&str>,
) -> Option<AlbumSearchAction> {
    match key {
        KeyCode::Esc => return Some(AlbumSearchAction::Back),
        KeyCode::Up => {
            if state.current_index > 0 {
                state.current_index -= 1;
            }
        }
        KeyCode::Down => {
            if state.current_index + 1 < state.albums.len() {
                state.current_index += 1;
            }
        }
        KeyCode::Char(' ') => {
            if let Some(pos) = state
                .selected_indices
                .iter()
                .position(|&x| x == state.current_index)
            {
                state.selected_indices.remove(pos);
            } else {
                state.selected_indices.push(state.current_index);
            }
        }
        KeyCode::Enter => {
            let selected = state.get_selected_albums();
            if !selected.is_empty() {
                let (_, quality) = state.quality_options[state.quality_index];
                return Some(AlbumSearchAction::Download(
                    selected,
                    DownloadConfig { quality },
                ));
            } else {
                state.message = "请先按空格选择专辑".to_string();
            }
        }
            KeyCode::Char('k') | KeyCode::Char('K') => {
            state.quality_index = (state.quality_index + 1) % state.quality_options.len();
        }
        _ => {}
    }
    None
}
