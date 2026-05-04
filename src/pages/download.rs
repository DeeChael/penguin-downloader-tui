use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex, OnceLock,
};

use penguin_downloader::{
    model::{
        DownloadComplete, DownloadError, DownloadProgress as DlProgress, DownloadStart,
        SongInfo,
    },
    DownloadCallbacks, DownloadOptions, Downloader, Error,
};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
    Frame,
};
use tokio::sync::Semaphore;
use tracing::{info, warn};

static REALTIME_PROGRESS: OnceLock<Arc<Mutex<HashMap<String, f32>>>> = OnceLock::new();

fn init_progress_map() -> Arc<Mutex<HashMap<String, f32>>> {
    REALTIME_PROGRESS
        .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

fn on_start_cb(p: &DownloadStart) {
    if let Some(map) = REALTIME_PROGRESS.get() {
        map.lock().unwrap().insert(p.song.id.clone(), 0.0);
    }
}

fn on_progress_cb(p: &DlProgress) {
    if let Some(map) = REALTIME_PROGRESS.get() {
        let pct = match p.total_size {
            Some(total) if total > 0 => p.downloaded as f32 / total as f32,
            _ => 0.0,
        };
        map.lock().unwrap().insert(p.song.id.clone(), pct);
    }
}

fn on_complete_cb(p: &DownloadComplete) {
    if let Some(map) = REALTIME_PROGRESS.get() {
        map.lock().unwrap().insert(p.song.id.clone(), 1.0);
    }
}

fn on_error_cb(p: &DownloadError) {
    if let Some(map) = REALTIME_PROGRESS.get() {
        map.lock().unwrap().insert(p.song.id.clone(), -1.0);
    }
}

#[derive(Clone)]
pub enum ItemStatus {
    Pending,
    Downloading,
    Complete,
    Existing,
    Error(String),
}

pub struct DownloadItem {
    pub idx: usize,
    pub title: String,
    pub artist: String,
    pub song_id: String,
    pub status: ItemStatus,
    pub progress: f32,
}

pub struct DownloadPageState {
    pub items: Arc<Mutex<Vec<DownloadItem>>>,
    pub total: usize,
    pub completed: Arc<AtomicUsize>,
    pub finished: Arc<AtomicBool>,
    pub cancelled: Arc<AtomicBool>,
    pub interrupted: Arc<AtomicBool>,
    pub provider_name: String,
    pub task_name: String,
    pub threads: usize,
    pub quality_name: String,
    pub output_dir: String,
}

impl DownloadPageState {
    pub fn new(
        provider_name: String,
        task_name: String,
        total: usize,
        threads: usize,
        quality_name: String,
        output_dir: String,
    ) -> Self {
        let items = (0..total)
            .map(|i| DownloadItem {
                idx: i,
                title: String::new(),
                artist: String::new(),
                song_id: String::new(),
                status: ItemStatus::Pending,
                progress: 0.0,
            })
            .collect();

        Self {
            items: Arc::new(Mutex::new(items)),
            total,
            completed: Arc::new(AtomicUsize::new(0)),
            finished: Arc::new(AtomicBool::new(false)),
            cancelled: Arc::new(AtomicBool::new(false)),
            interrupted: Arc::new(AtomicBool::new(false)),
            provider_name,
            task_name,
            threads,
            quality_name,
            output_dir,
        }
    }

    pub fn update_item(&self, idx: usize, title: String, artist: String, song_id: String) {
        if let Ok(mut items) = self.items.lock() {
            if idx < items.len() {
                items[idx].title = title;
                items[idx].artist = artist;
                items[idx].song_id = song_id;
            }
        }
    }

    pub fn set_status(&self, idx: usize, status: ItemStatus) {
        if let Ok(mut items) = self.items.lock() {
            if idx < items.len() {
                items[idx].status = status;
            }
        }
    }

    pub fn set_progress(&self, idx: usize, progress: f32) {
        if let Ok(mut items) = self.items.lock() {
            if idx < items.len() {
                items[idx].progress = progress;
            }
        }
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn sync_progress(&self) {
        let Some(map) = REALTIME_PROGRESS.get() else { return };
        let progress_map = map.lock().unwrap();
        if let Ok(mut items) = self.items.lock() {
            for item in items.iter_mut() {
                if !item.song_id.is_empty() {
                    if let Some(&pct) = progress_map.get(&item.song_id) {
                        item.progress = pct.max(0.0);
                    }
                }
            }
        }
    }
}

pub async fn execute_download(
    downloader: Arc<Downloader>,
    songs: Vec<SongInfo>,
    options: DownloadOptions,
    state: Arc<DownloadPageState>,
    output_dir: std::path::PathBuf,
    threads: usize,
    quality_levels: Vec<i32>,
) {
    let total = songs.len();
    let progress_map = init_progress_map();

    info!(
        "[下载] 开始批量下载: {} 首, 线程={}, 初始音质={}, 输出={:?}, fallback={:?}",
        total, threads, options.quality, output_dir, quality_levels
    );

    {
        let mut map = progress_map.lock().unwrap();
        for song in &songs {
            map.insert(song.id.clone(), 0.0);
        }
    }

    let base_opts = options;
    let semaphore = Arc::new(Semaphore::new(threads));
    let mut handles = Vec::new();

    for (idx, song) in songs.into_iter().enumerate() {
        let sem = semaphore.clone();
        let dl = downloader.clone();
        let st = state.clone();
        let out = output_dir.clone();
        let levels = quality_levels.clone();
        let song_id = song.id.clone();
        let pm = progress_map.clone();
        let bo = base_opts.clone();

        let artist_str = if song.artists.is_empty() {
            String::new()
        } else {
            song.artists.join(" / ")
        };

        st.update_item(idx, song.title.clone(), artist_str, song.id.clone());

        let handle = tokio::spawn(async move {
            // Check if cancelled before acquiring semaphore
            if st.is_cancelled() {
                pm.lock().unwrap().remove(&song_id);
                return;
            }

            let _permit = sem.acquire().await.unwrap();

            // Check again after acquiring - still not cancelled?
            if st.is_cancelled() {
                pm.lock().unwrap().remove(&song_id);
                return;
            }

            st.set_status(idx, ItemStatus::Downloading);

            let mut last_error = String::new();
            let mut success = false;

            for (attempt, &ql) in levels.iter().enumerate() {
                if st.is_cancelled() { break; }

                let mut try_opts = bo.clone();
                try_opts.quality = ql;
                try_opts.callbacks = DownloadCallbacks::new()
                    .with_start(on_start_cb)
                    .with_progress(on_progress_cb)
                    .with_complete(on_complete_cb)
                    .with_error(on_error_cb);

                info!(
                    "[下载] 尝试 [{}/{}] id={} title={} 音质={} ({}/{})",
                    idx + 1, total, song.id, song.title, ql, attempt + 1, levels.len()
                );

                match dl.download_song_to_dir(&song, &try_opts, &out).await {
                    Ok(path) => {
                        info!("[下载] 下载完成 [{}/{}] 音质={}: {:?}", idx + 1, total, ql, path);
                        st.set_status(idx, ItemStatus::Complete);
                        st.set_progress(idx, 1.0);
                        st.completed.fetch_add(1, Ordering::SeqCst);
                        success = true;
                        pm.lock().unwrap().remove(&song_id);
                        break;
                    }
                    Err(e) => {
                        // Check for AlreadyExists BEFORE string conversion
                        if matches!(&e, Error::AlreadyExists { .. }) {
                            info!("[下载] 文件已存在 [{}/{}]: {:?}", idx + 1, total, song.title);
                            st.set_status(idx, ItemStatus::Existing);
                            st.set_progress(idx, 1.0);
                            st.completed.fetch_add(1, Ordering::SeqCst);
                            success = true;
                            pm.lock().unwrap().remove(&song_id);
                            break;
                        }
                        last_error = format!("{:?}", e);
                        warn!("[下载] 音质 {} 失败: {:?}", ql, e);
                        if !matches!(e, Error::NoDataExists) {
                            break;
                        }
                    }
                }
            }

            if !success && !st.is_cancelled() {
                warn!("[下载] 下载失败 [{}/{}] id={} title={}: {}", idx + 1, total, song.id, song.title, last_error);
                st.set_status(idx, ItemStatus::Error(last_error));
                st.completed.fetch_add(1, Ordering::SeqCst);
                pm.lock().unwrap().remove(&song_id);
            }
        });
        handles.push(handle);
    }

    for h in handles {
        let _ = h.await;
    }

    let completed = state.completed.load(Ordering::SeqCst);
    info!("[下载] 所有下载任务完成: {}/{}", completed, state.total);

    // Clean up partial files for cancelled downloads
    if state.is_cancelled() {
        let out_dir = std::path::Path::new(&output_dir);
        if out_dir.exists() {
            // Delete all files that were being downloaded (now partial)
            if let Ok(entries) = std::fs::read_dir(out_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        // Files being downloaded typically have tmp extensions or are newly created
                        // Just clean up known audio extensions
                        if let Some(ext) = path.extension() {
                            let ext_str = ext.to_string_lossy().to_lowercase();
                            if ["mp3", "flac", "m4a", "ogg", "lrc"].contains(&ext_str.as_str()) {
                                // Only delete small files (likely partial downloads)
                                if let Ok(meta) = std::fs::metadata(&path) {
                                    // Files < 1MB are likely partial or just headers
                                    // Better approach: check if the file is in our download list
                                    // For now, skip the auto-cleanup to avoid deleting existing files
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    state.finished.store(true, Ordering::SeqCst);
}

pub fn render_download_ui(frame: &mut Frame, state: &DownloadPageState) {
    state.sync_progress();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(3), Constraint::Length(3), Constraint::Min(5)])
        .split(frame.area());

    let info_lines = vec![
        Line::from(vec![
            Span::styled("音源: ", Style::default().fg(Color::Cyan)), Span::raw(&state.provider_name),
            Span::raw("  │  "),
            Span::styled("线程: ", Style::default().fg(Color::Cyan)), Span::raw(state.threads.to_string()),
            Span::raw("  │  "),
            Span::styled("音质: ", Style::default().fg(Color::Cyan)), Span::raw(&state.quality_name),
        ]),
        Line::from(vec![Span::styled("保存到: ", Style::default().fg(Color::Cyan)), Span::raw(&state.output_dir)]),
    ];

    frame.render_widget(
        Paragraph::new(info_lines).block(Block::default().borders(Borders::ALL)),
        chunks[0],
    );

    let completed = state.completed.load(Ordering::SeqCst);
    let total = state.total;
    let (ratio, label) = if total == 1 {
        let p = {
            let items = state.items.lock().unwrap();
            items.first().map(|i| i.progress).unwrap_or(0.0)
        };
        let pct = (p * 100.0) as u16;
        (p as f64, format!("{:3}%", pct.min(100)))
    } else {
        let r = if total > 0 { completed as f64 / total as f64 } else { 0.0 };
        let pct = (r * 100.0) as u16;
        (r, format!("{:3}%  ({}/{})", pct.min(100), completed, total))
    };

    frame.render_widget(
        Gauge::default()
            .block(Block::default().title("总体进度").borders(Borders::ALL))
            .gauge_style(Style::default().fg(Color::Green))
            .label(label)
            .ratio(ratio),
        chunks[1],
    );

    let items = state.items.lock().unwrap();
    let mut sorted: Vec<&DownloadItem> = items.iter().collect();
    sorted.sort_by_key(|item| match item.status {
        ItemStatus::Downloading => 0,
        ItemStatus::Pending => 1,
        _ => 2,
    });

    let term_w = frame.area().width as usize;
    let bar_w = (term_w / 3).max(8).min(40);

    let mut lines = Vec::new();
    for item in sorted {
        let (icon, color) = match &item.status {
            ItemStatus::Complete => ("✓", Color::Green),
            ItemStatus::Existing => ("•", Color::Blue),
            ItemStatus::Error(_) => ("✗", Color::Red),
            ItemStatus::Downloading => ("↓", Color::Yellow),
            ItemStatus::Pending => (" ", Color::Gray),
        };

        if matches!(item.status, ItemStatus::Downloading) {
            let pct = (item.progress * 100.0) as u16;
            let filled = (item.progress * bar_w as f32) as usize;
            let text = format!(" {} {} - {}", icon, item.title, item.artist);
            lines.push(Line::from(Span::styled(
                format!("{} [{}{}] {:3}%", truncate(&text, term_w.saturating_sub(bar_w + 8)), "█".repeat(filled), "░".repeat(bar_w.saturating_sub(filled)), pct.min(100)),
                Style::default().fg(color),
            )));
        } else {
            let text = match &item.status {
                ItemStatus::Error(msg) => format!(" {} {} - {}: {}", icon, item.title, item.artist, msg),
                _ => format!(" {} {} - {}", icon, item.title, item.artist),
            };
            lines.push(Line::from(Span::styled(truncate(&text, term_w), Style::default().fg(color))));
        }
    }

    frame.render_widget(
        Paragraph::new(lines).block(Block::default().title("详情").borders(Borders::ALL)),
        chunks[2],
    );
}

pub fn render_completion_page(frame: &mut Frame, state: &DownloadPageState) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(5), Constraint::Min(3), Constraint::Length(3)])
        .split(area);

    let interrupted = state.interrupted.load(Ordering::SeqCst);
    let title_text = if interrupted { " 下载中断  " } else { " 下载完成  " };
    let title_color = if interrupted { Color::Yellow } else { Color::Green };

    let items = state.items.lock().unwrap();
    let total = items.len();
    let mut success = 0;
    let mut existing = 0;
    let mut failed = 0;
    let mut skipped = 0;

    for item in items.iter() {
        match &item.status {
            ItemStatus::Complete => success += 1,
            ItemStatus::Existing => existing += 1,
            ItemStatus::Error(_) => failed += 1,
            ItemStatus::Pending => skipped += 1,
            _ => {}
        }
    }

    let stat_line = if skipped > 0 {
        format!("  ✓ 成功: {} 首  • 已存在: {} 首  ✗ 失败: {} 首  未下载: {} 首", success, existing, failed, skipped)
    } else {
        format!("  ✓ 成功: {} 首  • 已存在: {} 首  ✗ 失败: {} 首", success, existing, failed)
    };

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![Span::styled(title_text, Style::default().fg(title_color).add_modifier(Modifier::BOLD))]),
            Line::from(format!("  总计: {} 首", total)),
            Line::from(stat_line),
        ])
        .block(Block::default().title("统计").borders(Borders::ALL))
        .alignment(Alignment::Center),
        chunks[0],
    );

    let mut detail_lines = Vec::new();
    for item in items.iter() {
        let (icon, color) = match &item.status {
            ItemStatus::Complete => ("✓", Color::Green),
            ItemStatus::Existing => ("•", Color::Blue),
            ItemStatus::Error(_) => ("✗", Color::Red),
            _ => ("?", Color::Gray),
        };
        let text = match &item.status {
            ItemStatus::Error(msg) => format!(" {} {} - {}: {}", icon, item.title, item.artist, msg),
            _ => format!(" {} {} - {}", icon, item.title, item.artist),
        };
        detail_lines.push(Line::from(Span::styled(text, Style::default().fg(color))));
    }

    frame.render_widget(
        Paragraph::new(detail_lines).block(Block::default().title("详情").borders(Borders::ALL)),
        chunks[1],
    );

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("按任意键返回", Style::default().fg(Color::Gray))))
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center),
        chunks[2],
    );
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        format!("{}...", s.chars().take(max.saturating_sub(3)).collect::<String>())
    } else {
        s.to_string()
    }
}
