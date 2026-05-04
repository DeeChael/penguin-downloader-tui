use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use penguin_downloader::{provider::MusicProvider, DownloadOptions, PenguinCore};
use ratatui::{backend::Backend, Terminal};
use tracing::info;

use crate::config::Config;
use crate::credential::CredentialStore;
use crate::pages::{album_search, download, home, login, playlist, search, settings};

pub enum AppPage {
    Home,
    Search(Box<search::SearchPageState>),
    AlbumSearch(Box<album_search::AlbumSearchState>),
    Playlist(Box<playlist::PlaylistPageState>),
    Login(Box<login::LoginPageState>),
    Download(Arc<download::DownloadPageState>),
    Settings(Box<settings::SettingsState>),
}

pub struct App {
    pub core: PenguinCore,
    pub credential_store: CredentialStore,
    pub credential_path: PathBuf,
    pub _plugins_dir: PathBuf,
    pub output_dir: PathBuf,
    pub config: Config,
    pub config_path: PathBuf,
    pub page: AppPage,
    pub home_state: home::HomeState,
    pub running: bool,
    pub current_provider_name: String,
    pub current_provider_display: String,
}

impl App {
    pub fn new(
        core: PenguinCore,
        plugins_dir: PathBuf,
        credential_path: PathBuf,
        output_dir: PathBuf,
        config_path: PathBuf,
    ) -> Self {
        let credentials = CredentialStore::load(&credential_path);
        let config = Config::load_or_default(&config_path);
        let provider_names = core.list_provider_names();
        let current_provider = provider_names.first().cloned().unwrap_or_default();
        let current_display = core
            .get_provider(&current_provider)
            .map(|p| p.name().to_string())
            .unwrap_or_default();
        App {
            home_state: home::HomeState::new(provider_names),
            page: AppPage::Home,
            running: true,
            current_provider_name: current_provider,
            current_provider_display: current_display,
            credential_store: credentials,
            credential_path,
            _plugins_dir: plugins_dir,
            output_dir,
            config,
            config_path,
            core,
        }
    }

    pub fn get_credential(&self) -> Option<String> {
        self.credential_store
            .get_credential(&self.current_provider_name)
            .map(|s| s.to_string())
    }

    pub fn get_provider(&self) -> Option<Arc<dyn MusicProvider>> {
        self.core.get_provider(&self.current_provider_name)
    }

    pub fn save_credentials(&self) {
        if let Err(e) = self.credential_store.save(&self.credential_path) {
            tracing::error!("保存凭证失败: {}", e);
        }
    }
}

pub async fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> anyhow::Result<()> {
    while app.running {
        terminal.draw(|f| render(f, app))?;

        if let AppPage::Login(ref mut state) = app.page {
            login::poll_bg_login(state, &mut app.credential_store);
        }

        if !event::poll(Duration::from_millis(100))? {
            continue;
        }

        if let Event::Key(key_event) = event::read()? {
            if key_event.kind != KeyEventKind::Press {
                continue;
            }

            let key = key_event.code;
            let provider = app.get_provider();
            let credential = app.get_credential();

            match &mut app.page {
                AppPage::Home => {
                    handle_home_key(app, key_event).await;
                }
                AppPage::Search(ref mut state) => {
                    if let Some(ref p) = provider {
                        match search::handle_search_input(state, key, p, credential.as_deref()).await {
                            Some(search::SearchAction::Back) => app.page = AppPage::Home,
                            Some(search::SearchAction::Download(songs)) => {
                                let dl = app.core.get_downloader(p.clone(), app.get_credential());
                                start_song_download(app, dl, songs).await;
                            }
                            None => {}
                        }
                    } else {
                        app.page = AppPage::Home;
                    }
                }
                AppPage::AlbumSearch(ref mut state) => {
                    if let Some(ref p) = provider {
                        match album_search::handle_album_search_input(state, key, p, credential.as_deref()).await {
                            Some(album_search::AlbumSearchAction::Back) => app.page = AppPage::Home,
                            Some(album_search::AlbumSearchAction::Download(albums, _cfg)) => {
                                start_album_download(app, p.clone(), albums).await;
                            }
                            None => {}
                        }
                    } else {
                        app.page = AppPage::Home;
                    }
                }
                AppPage::Playlist(ref mut state) => {
                    if let Some(ref p) = provider {
                        match playlist::handle_playlist_input(state, key, p, credential.as_deref()).await {
                            Some(playlist::PlaylistAction::Back) => app.page = AppPage::Home,
                            Some(playlist::PlaylistAction::Download(pl, _cfg)) => {
                                start_playlist_download(app, p.clone(), pl).await;
                            }
                            None => {}
                        }
                    } else {
                        app.page = AppPage::Home;
                    }
                }
                AppPage::Login(ref mut state) => {
                    match login::handle_login_input(state, key, &mut app.credential_store, &app.core).await {
                        Some(login::LoginPageAction::Back) => {
                            app.save_credentials();
                            app.page = AppPage::Home;
                        }
                        None => {}
                    }
                }
                AppPage::Download(ref state) => {
                    if state.is_finished() {
                        if key == KeyCode::Esc || key == KeyCode::Enter || key == KeyCode::Char(' ') {
                            app.page = AppPage::Home;
                        }
                    } else if key == KeyCode::Esc {
                        state.cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
                        state.interrupted.store(true, std::sync::atomic::Ordering::SeqCst);
                        state.finished.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                }
                AppPage::Settings(ref mut state) => {
                    match settings::handle_settings_input(state, key).await {
                        Some(settings::SettingsAction::Back) => app.page = AppPage::Home,
                        Some(settings::SettingsAction::Saved) => {
                            app.config = Config::load_or_default(&app.config_path);
                            app.page = AppPage::Home;
                        }
                        None => {}
                    }
                }
            }
        }
    }
    Ok(())
}

fn render(frame: &mut ratatui::Frame, app: &mut App) {
    match &app.page {
        AppPage::Home => home::render_home(frame, &app.home_state, &app.credential_store, &app.current_provider_display),
        AppPage::Search(ref state) => search::render_search_ui(frame, state),
        AppPage::AlbumSearch(ref state) => album_search::render_album_search_ui(frame, state),
        AppPage::Playlist(ref state) => playlist::render_playlist_ui(frame, state),
        AppPage::Login(ref state) => login::render_login_page(frame, state, &app.credential_store),
        AppPage::Download(ref state) => {
            if state.is_finished() {
                download::render_completion_page(frame, state);
            } else {
                download::render_download_ui(frame, state);
            }
        }
        AppPage::Settings(ref state) => settings::render_settings(frame, state),
    }
}

async fn handle_home_key(app: &mut App, key_event: KeyEvent) {
    let key = key_event.code;
    let modifiers = key_event.modifiers;

    if app.home_state.show_clean_confirm {
        match key {
            KeyCode::Left | KeyCode::Right => app.home_state.clean_confirm_yes = !app.home_state.clean_confirm_yes,
            KeyCode::Enter => {
                app.home_state.show_clean_confirm = false;
                if app.home_state.clean_confirm_yes { do_cleanup(app); }
            }
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => app.home_state.show_clean_confirm = false,
            KeyCode::Char('y') | KeyCode::Char('Y') => { app.home_state.show_clean_confirm = false; do_cleanup(app); }
            _ => {}
        }
        return;
    }

    match key {
        KeyCode::Esc => { app.save_credentials(); app.running = false; }
        KeyCode::Tab => {
            app.home_state.search_type = match app.home_state.search_type {
                home::SearchType::Song => home::SearchType::Album,
                home::SearchType::Album => home::SearchType::Song,
            };
        }
        KeyCode::Up => app.home_state.focus_prev(),
        KeyCode::Down => app.home_state.focus_next(),
        KeyCode::Char('P') | KeyCode::Char('p') if modifiers.contains(KeyModifiers::CONTROL) => {
            app.home_state.cycle_provider();
            let names = app.core.list_provider_names();
            if !names.is_empty() {
                let idx = app.home_state.current_provider_index.min(names.len() - 1);
                app.current_provider_name = names[idx].clone();
                app.current_provider_display = app.core.get_provider(&app.current_provider_name)
                    .map(|p| p.name().to_string()).unwrap_or_default();
            }
            app.home_state.message = format!("当前音源: {}", app.current_provider_display);
        }
        KeyCode::Enter => {
            if app.home_state.focus == home::HomeFocus::SearchBox {
                if !app.home_state.search_text.trim().is_empty() {
                    let kw = app.home_state.search_text.trim().to_string();
                    app.home_state.search_text.clear();
                    app.home_state.cursor_pos = 0;

                    if let Some(p) = app.get_provider() {
                        if p.requires_login() && app.get_credential().is_none() {
                            app.home_state.message = format!("{}需要登录才能搜索，请先到登录管理登录", app.current_provider_display);
                            return;
                        }
                    } else {
                        app.home_state.message = "没有可用的音源".to_string();
                        return;
                    }

                    let p = app.get_provider().unwrap();
                    match app.home_state.search_type {
                        home::SearchType::Song => {
                            let mut s = search::SearchPageState::new(p.name().to_string(), p.as_ref());
                            s.keyword = kw.clone(); s.loading = true;
                            let cred = app.get_credential();
                            match search::do_search(&p, &kw, 1, cred.as_deref()).await {
                                Ok(r) if r.is_success() && !r.songs.is_empty() => {
                                    s.songs = r.songs; s.loading = false;
                                }
                                Ok(_) => { s.loading = false; s.message = "未找到结果".to_string(); }
                                Err(e) => { s.loading = false; s.message = format!("搜索失败: {}", e); }
                            }
                            app.page = AppPage::Search(Box::new(s));
                        }
                        home::SearchType::Album => {
                            let mut s = album_search::AlbumSearchState::new(p.name().to_string(), p.as_ref());
                            s.keyword = kw.clone(); s.loading = true; s.message = String::new();
                            let cred = app.get_credential();
                            match album_search::do_album_search(&p, &kw, 1, cred.as_deref()).await {
                                Ok(Some(r)) if r.is_success() && !r.albums.is_empty() => { s.albums = r.albums; }
                                Ok(_) => { s.message = "未找到专辑".to_string(); }
                                Err(e) => { s.message = format!("搜索失败: {}", e); }
                            }
                            s.loading = false;
                            app.page = AppPage::AlbumSearch(Box::new(s));
                        }
                    }
                }
            } else if app.home_state.focus == home::HomeFocus::Playlists {
                if let Some(p) = app.get_provider() {
                    if p.requires_login() && app.get_credential().is_none() {
                        app.home_state.message = format!("{}需要登录才能下载歌单，请先到登录管理登录", app.current_provider_display);
                        return;
                    }
                    let mut state = playlist::PlaylistPageState::new(p.name().to_string(), p.as_ref());
                    let cred = app.get_credential(); state.loading = true;
                    app.page = AppPage::Playlist(Box::new(state));
                    if let AppPage::Playlist(ref mut ps) = app.page {
                        match p.get_user_playlists(cred.as_deref()).await {
                            Ok(list) if !list.is_empty() => { ps.playlists = list; ps.loading = false; ps.message = String::new(); }
                            Ok(_) => { ps.loading = false; ps.message = "没有找到歌单".to_string(); }
                            Err(e) => { ps.loading = false; ps.message = format!("加载失败: {}", e); }
                        }
                    }
                } else { app.home_state.message = "没有可用的音源".to_string(); }
            } else if app.home_state.focus == home::HomeFocus::Credential {
                let providers = app.core.list_providers();
                if providers.is_empty() { app.home_state.message = "没有可用的音源".to_string(); }
                else { app.page = AppPage::Login(Box::new(login::LoginPageState::new(providers, &app.credential_store))); }
            } else if app.home_state.focus == home::HomeFocus::CleanFiles {
                app.home_state.show_clean_confirm = true;
            } else if app.home_state.focus == home::HomeFocus::Settings {
                app.page = AppPage::Settings(Box::new(settings::SettingsState::new(&app.config, app.config_path.clone())));
            }
        }
        KeyCode::Char(c) if !modifiers.contains(KeyModifiers::CONTROL) => {
            if app.home_state.focus == home::HomeFocus::SearchBox {
                app.home_state.insert_char(c);
            }
        }
        KeyCode::Backspace => { if app.home_state.focus == home::HomeFocus::SearchBox { app.home_state.delete_char(); } }
        KeyCode::Left => { if app.home_state.focus == home::HomeFocus::SearchBox { app.home_state.cursor_left(modifiers.contains(KeyModifiers::SHIFT)); } }
        KeyCode::Right => { if app.home_state.focus == home::HomeFocus::SearchBox { app.home_state.cursor_right(modifiers.contains(KeyModifiers::SHIFT)); } }
        KeyCode::Home => { if app.home_state.focus == home::HomeFocus::SearchBox { app.home_state.home_key(modifiers.contains(KeyModifiers::SHIFT)); } }
        KeyCode::End => { if app.home_state.focus == home::HomeFocus::SearchBox { app.home_state.end_key(modifiers.contains(KeyModifiers::SHIFT)); } }
        KeyCode::Char('a') | KeyCode::Char('A') if modifiers.contains(KeyModifiers::CONTROL) => {
            if app.home_state.focus == home::HomeFocus::SearchBox { app.home_state.select_all(); }
        }
        _ => {}
    }
}

fn do_cleanup(app: &mut App) {
    match clean_download_dir(&app.output_dir) {
        Ok(_) => app.home_state.message = "已清理所有下载文件".to_string(),
        Err(e) => app.home_state.message = format!("清理失败: {}", e),
    }
}

fn clean_download_dir(dir: &std::path::Path) -> std::io::Result<()> {
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
        std::fs::create_dir_all(dir)?;
        info!("已清理下载目录: {:?}", dir);
    }
    Ok(())
}

fn resolve_quality(provider: &dyn MusicProvider, quality: i32) -> i32 {
    if quality == -1 { provider.get_highest_quality() } else { quality }
}

fn quality_name(provider: &dyn MusicProvider, quality: i32) -> String {
    let q = resolve_quality(provider, quality);
    provider.quality_levels().get(&q).cloned().unwrap_or_else(|| format!("品质{}", q))
}

fn sorted_quality_levels(provider: &dyn MusicProvider) -> Vec<i32> {
    let mut levels: Vec<i32> = provider.quality_levels().keys().copied().collect();
    levels.sort_by(|a, b| b.cmp(a));
    levels
}

fn sanitize_path(name: &str) -> String {
    let invalid = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    name.chars().map(|c| if invalid.contains(&c) { '_' } else { c }).collect::<String>().trim().to_string()
}

fn build_download_opts(config: &Config, quality: i32) -> DownloadOptions {
    let mut opts = DownloadOptions::new().with_quality(quality);
    opts.lyric_type = match config.lyrics.r#type.as_str() {
        "normal" => penguin_downloader::model::LyricType::Normal,
        "verbatim" => penguin_downloader::model::LyricType::Verbatim,
        _ => penguin_downloader::model::LyricType::None,
    };
    if config.lyrics.r#type != "none" {
        opts.lyric_translation = config.lyrics.translation;
        opts.lyric_romanization = config.lyrics.roma;
    }
    opts
}

async fn start_song_download(
    app: &mut App,
    downloader: penguin_downloader::Downloader,
    songs: Vec<penguin_downloader::model::SongInfo>,
) {
    let total = songs.len();
    let p = app.get_provider().unwrap();
    let actual_q = resolve_quality(p.as_ref(), -1);
    let qn = quality_name(p.as_ref(), -1);
    let pn = p.name().to_string();
    let has_cred = app.get_credential().is_some();
    info!("[下载] start_song_download: {} 首, provider={}, quality=-1(实际={}), 有凭证={}", total, pn, actual_q, has_cred);
    let songs_dir = app.output_dir.join("songs");
    let out_str = songs_dir.display().to_string();
    let threads = app.config.threads as usize;
    let state = Arc::new(download::DownloadPageState::new(pn, tn(total, &songs), total, threads, qn, out_str));
    let mut opts = build_download_opts(&app.config, actual_q);
    opts.format = Some("{title} - {artist}".to_string());
    let levels = sorted_quality_levels(p.as_ref());
    let ds = state.clone(); let out = songs_dir; let arc_dl = Arc::new(downloader);
    tokio::spawn(async move { download::execute_download(arc_dl, songs, opts, ds, out, threads, levels).await; });
    app.page = AppPage::Download(state);
}

async fn start_album_download(
    app: &mut App,
    provider: Arc<dyn MusicProvider>,
    albums: Vec<penguin_downloader::model::AlbumInfo>,
) {
    for album in albums {
        let album_dir = app.output_dir.join("albums").join(sanitize_path(&album.title));
        let actual_q = resolve_quality(provider.as_ref(), -1);
        let cred = app.get_credential();

        let songs = match provider
            .get_album_songs(&album.id, penguin_downloader::provider::Pagination::default_list(), cred.as_deref())
            .await
        {
            Ok(songs) if !songs.is_empty() => songs,
            Ok(_) => {
                let state = Arc::new(download::DownloadPageState::new(
                    provider.name().to_string(), album.title.clone(), 1, 1,
                    quality_name(provider.as_ref(), -1), album_dir.display().to_string(),
                ));
                state.update_item(0, format!("专辑无歌曲: {}", album.title), String::new(), String::new());
                state.set_status(0, download::ItemStatus::Error("该专辑没有可下载的歌曲".to_string()));
                state.completed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                state.finished.store(true, std::sync::atomic::Ordering::SeqCst);
                app.page = AppPage::Download(state); return;
            }
            Err(e) => {
                let state = Arc::new(download::DownloadPageState::new(
                    provider.name().to_string(), album.title.clone(), 1, 1,
                    quality_name(provider.as_ref(), -1), album_dir.display().to_string(),
                ));
                state.update_item(0, format!("获取失败: {}", album.title), String::new(), String::new());
                state.set_status(0, download::ItemStatus::Error(format!("{}", e)));
                state.completed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                state.finished.store(true, std::sync::atomic::Ordering::SeqCst);
                app.page = AppPage::Download(state); return;
            }
        };

        let qn = quality_name(provider.as_ref(), -1);
        let pn = provider.name().to_string();
        let total = songs.len();
        let threads = app.config.threads as usize;
        let state = Arc::new(download::DownloadPageState::new(pn, album.title.clone(), total, threads, qn, album_dir.display().to_string()));
        let mut opts = build_download_opts(&app.config, actual_q);
        opts.format = Some("{track} {title}".to_string());
        let levels = sorted_quality_levels(provider.as_ref());
        let dl = app.core.get_downloader(provider.clone(), cred);
        let ds = state.clone(); let out = album_dir; let arc_dl = Arc::new(dl);
        tokio::spawn(async move { download::execute_download(arc_dl, songs, opts, ds, out, threads, levels).await; });
        app.page = AppPage::Download(state); return;
    }
}

async fn start_playlist_download(
    app: &mut App,
    provider: Arc<dyn MusicProvider>,
    playlists: Vec<penguin_downloader::model::UserPlaylist>,
) {
    for pl in playlists {
        let pl_dir = app.output_dir.join("playlists").join(sanitize_path(&pl.title));
        let actual_q = resolve_quality(provider.as_ref(), -1);
        let cred = app.get_credential();

        let result = match provider
            .get_playlist_songs(&pl.id, penguin_downloader::provider::Pagination::default_list(), cred.as_deref())
            .await
        {
            Ok(r) if !r.songs.is_empty() => r,
            Ok(_) => {
                let state = Arc::new(download::DownloadPageState::new(
                    provider.name().to_string(), pl.title.clone(), 1, 1,
                    quality_name(provider.as_ref(), -1), pl_dir.display().to_string(),
                ));
                state.update_item(0, format!("歌单无歌曲: {}", pl.title), String::new(), String::new());
                state.set_status(0, download::ItemStatus::Error("该歌单没有可下载的歌曲".to_string()));
                state.completed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                state.finished.store(true, std::sync::atomic::Ordering::SeqCst);
                app.page = AppPage::Download(state); return;
            }
            Err(e) => {
                let state = Arc::new(download::DownloadPageState::new(
                    provider.name().to_string(), pl.title.clone(), 1, 1,
                    quality_name(provider.as_ref(), -1), pl_dir.display().to_string(),
                ));
                state.update_item(0, format!("获取失败: {}", pl.title), String::new(), String::new());
                state.set_status(0, download::ItemStatus::Error(format!("{}", e)));
                state.completed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                state.finished.store(true, std::sync::atomic::Ordering::SeqCst);
                app.page = AppPage::Download(state); return;
            }
        };

        let qn = quality_name(provider.as_ref(), -1);
        let pn = provider.name().to_string();
        let songs = result.songs;
        let total = songs.len();
        let threads = app.config.threads as usize;
        let state = Arc::new(download::DownloadPageState::new(pn, result.title.clone(), total, threads, qn, pl_dir.display().to_string()));
        let mut opts = build_download_opts(&app.config, actual_q);
        opts.format = Some("{title} - {artist}".to_string());
        let levels = sorted_quality_levels(provider.as_ref());
        let dl = app.core.get_downloader(provider.clone(), cred);
        let ds = state.clone(); let out = pl_dir; let arc_dl = Arc::new(dl);
        tokio::spawn(async move { download::execute_download(arc_dl, songs, opts, ds, out, threads, levels).await; });
        app.page = AppPage::Download(state); return;
    }
}

fn tn(total: usize, songs: &[penguin_downloader::model::SongInfo]) -> String {
    if total <= 3 {
        songs.iter().map(|s| s.title.clone()).collect::<Vec<_>>().join("、")
    } else {
        format!("{} 等", songs.iter().take(3).map(|s| s.title.clone()).collect::<Vec<_>>().join("、"))
    }
}
