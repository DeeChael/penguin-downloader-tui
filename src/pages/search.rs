use std::collections::HashSet;
use std::sync::Arc;

use crossterm::event::KeyCode;
use penguin_downloader::{
    model::{SearchResult, SongInfo},
    provider::{MusicProvider, Pagination},
};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame,
};

pub enum SearchAction {
    Back,
    Download(Vec<SongInfo>),
}

pub struct SearchPageState {
    pub keyword: String,
    pub provider_name: String,
    pub songs: Vec<SongInfo>,
    pub selected_index: usize,
    pub selected_songs: HashSet<usize>,
    pub page: i32,
    pub loading: bool,
    pub message: String,
    pub quality_index: usize,
    pub quality_options: Vec<(String, i32)>,
}

impl SearchPageState {
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
            songs: Vec::new(),
            selected_index: 0,
            selected_songs: HashSet::new(),
            page: 1,
            loading: true,
            message: "搜索中...".to_string(),
            quality_index: quality_options.len() - 1,
            quality_options,
        }
    }

    pub fn get_selected_songs(&self) -> Vec<SongInfo> {
        self.selected_songs
            .iter()
            .filter(|&&idx| idx < self.songs.len())
            .map(|&idx| self.songs[idx].clone())
            .collect()
    }
}

pub fn render_search_ui(frame: &mut Frame, state: &SearchPageState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(frame.area());

    let title = format!("搜索结果: {} [{}]  (Esc 返回)", state.keyword, state.provider_name);
    frame.render_widget(Paragraph::new(title).block(Block::default().borders(Borders::ALL)), chunks[0]);

    if state.loading {
        frame.render_widget(Paragraph::new("搜索中...").block(Block::default().borders(Borders::ALL)), chunks[1]);
    } else if state.songs.is_empty() {
        frame.render_widget(Paragraph::new(state.message.clone()).block(Block::default().borders(Borders::ALL)), chunks[1]);
    } else {
        let header = Row::new(vec![
            Cell::from("选择").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("歌曲名称").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("艺术家").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("专辑").style(Style::default().add_modifier(Modifier::BOLD)),
        ]).style(Style::default().fg(Color::Yellow));

        let rows: Vec<Row> = state.songs.iter().enumerate().map(|(i, song)| {
            let sel_row = i == state.selected_index;
            let checked = state.selected_songs.contains(&i);
            let artist = if song.artists.is_empty() { "未知".to_string() } else { song.artists.join(" / ") };
            Row::new(vec![
                Cell::from(if checked { "[✓]" } else { "[ ]" }),
                Cell::from(truncate(&song.title, 30)),
                Cell::from(truncate(&artist, 20)),
                Cell::from(truncate(song.album.as_deref().unwrap_or("未知"), 25)),
            ]).style(if sel_row { Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD) }
                else if checked { Style::default().fg(Color::Green) } else { Style::default() })
        }).collect();

        frame.render_widget(
            Table::new(rows, [Constraint::Length(6), Constraint::Length(32), Constraint::Length(22), Constraint::Length(27)])
                .header(header)
                .block(Block::default().title(format!("歌曲列表 (第 {} 页)", state.page)).borders(Borders::ALL)),
            chunks[1],
        );
    }

    let selected_count = state.selected_songs.len();
    let q_spans: Vec<Span> = {
        let mut spans = vec![Span::raw("音质: ")];
        for (i, (name, _)) in state.quality_options.iter().enumerate() {
            if i > 0 { spans.push(Span::raw(" │ ")); }
            if i == state.quality_index {
                spans.push(Span::styled(format!("[{}]", name), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
            } else {
                spans.push(Span::styled(name.clone(), Style::default().fg(Color::Gray)));
            }
        }
        spans
    };

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(if selected_count > 0 { format!("已选择 {} 首", selected_count) } else { "未选择".to_string() },
                    if selected_count > 0 { Style::default().fg(Color::Green).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::Gray) }),
            ]),
            Line::from(q_spans),
        ]).block(Block::default()),
        chunks[2],
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("↑↓", Style::default().fg(Color::Yellow)), Span::raw(" 移动  "),
            Span::styled("Space", Style::default().fg(Color::Yellow)), Span::raw(" 选择  "),
            Span::styled("Enter", Style::default().fg(Color::Yellow)), Span::raw(" 下载  "),
            Span::styled("K", Style::default().fg(Color::Yellow)), Span::raw(" 音质  "),
            Span::styled("N/M", Style::default().fg(Color::Yellow)), Span::raw(" 翻页  "),
            Span::styled("Esc", Style::default().fg(Color::Yellow)), Span::raw(" 返回"),
        ])).block(Block::default().borders(Borders::ALL)),
        chunks[3],
    );
}

fn truncate(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max { s.to_string() } else { chars.iter().take(max.saturating_sub(3)).collect::<String>() + "..." }
}

pub async fn do_search(provider: &Arc<dyn MusicProvider>, keyword: &str, page: i32, credential: Option<&str>) -> anyhow::Result<SearchResult> {
    Ok(provider.search_songs(keyword, Pagination::new(20, page), credential).await?)
}

pub async fn handle_search_input(state: &mut SearchPageState, key: KeyCode, provider: &Arc<dyn MusicProvider>, credential: Option<&str>) -> Option<SearchAction> {
    match key {
        KeyCode::Esc => return Some(SearchAction::Back),
        KeyCode::Up => { if state.selected_index > 0 { state.selected_index -= 1; } }
        KeyCode::Down => { if state.selected_index + 1 < state.songs.len() { state.selected_index += 1; } }
        KeyCode::Char(' ') => {
            if state.selected_index < state.songs.len() {
                if state.selected_songs.contains(&state.selected_index) { state.selected_songs.remove(&state.selected_index); }
                else { state.selected_songs.insert(state.selected_index); }
            }
        }
        KeyCode::Enter => {
            if !state.selected_songs.is_empty() {
                return Some(SearchAction::Download(state.get_selected_songs()));
            } else { state.message = "请先按空格选择歌曲".to_string(); }
        }
        KeyCode::Char('k') | KeyCode::Char('K') => { state.quality_index = (state.quality_index + 1) % state.quality_options.len(); }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            if state.page > 1 {
                state.page -= 1; state.loading = true; state.selected_index = 0; state.selected_songs.clear();
                match do_search(provider, &state.keyword, state.page, credential).await {
                    Ok(r) if r.is_success() && !r.songs.is_empty() => { state.songs = r.songs; state.loading = false; }
                    Ok(_) => { state.loading = false; state.page += 1; state.message = "该页无结果".to_string(); }
                    Err(e) => { state.loading = false; state.page += 1; state.message = format!("加载失败: {}", e); }
                }
            }
        }
        KeyCode::Char('m') | KeyCode::Char('M') => {
            state.page += 1; state.loading = true; state.selected_index = 0; state.selected_songs.clear();
            match do_search(provider, &state.keyword, state.page, credential).await {
                Ok(r) if r.is_success() && !r.songs.is_empty() => { state.songs = r.songs; state.loading = false; }
                Ok(_) => { state.loading = false; state.page -= 1; state.message = "没有更多结果".to_string(); }
                Err(e) => { state.loading = false; state.page -= 1; state.message = format!("加载失败: {}", e); }
            }
        }
        _ => {}
    }
    None
}
