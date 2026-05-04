mod app;
mod config;
mod credential;
mod logging;
mod login_util;
mod pages;
mod qr_renderer;

use std::io::IsTerminal;
use std::path::PathBuf;

use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use penguin_downloader::PenguinCore;
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::app::App;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // If not in a terminal, spawn a new terminal window
    if !std::io::stdout().is_terminal() {
        let exe = std::env::current_exe().ok();
        if let Some(exe_path) = exe {
            let path = exe_path.to_string_lossy();
            #[cfg(target_os = "windows")]
            let _ = std::process::Command::new("cmd")
                .args(["/c", "start", "", &path])
                .spawn();
            #[cfg(target_os = "macos")]
            let _ = std::process::Command::new("open")
                .args(["-a", "Terminal", &path])
                .spawn();
            #[cfg(target_os = "linux")]
            {
                // Try common terminal emulators
                let terms = ["x-terminal-emulator", "gnome-terminal", "konsole", "xfce4-terminal", "lxterminal", "xterm"];
                for term in &terms {
                    if let Ok(_) = std::process::Command::new(term).arg("--").arg(&path).spawn() {
                        break;
                    }
                }
            }
            #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
            let _ = std::process::Command::new("xterm").arg("-e").arg(&path).spawn();
        }
        return Ok(());
    }

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let panic_msg = format!("程序发生 panic: {:?}", info);
        let _ = std::fs::write("panic.log", &panic_msg);
        eprintln!("{}", panic_msg);
        let _ = disable_raw_mode();
        let _ = execute!(
            std::io::stdout(),
            LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture
        );
        original_hook(info);
    }));

    let plugins_dir = PathBuf::from("plugins");
    let logs_dir = PathBuf::from("logs");
    let credential_path = PathBuf::from("credentials.json");
    let config_path = PathBuf::from("config.toml");
    let output_dir = PathBuf::from("downloads");

    std::fs::create_dir_all(&plugins_dir).ok();
    std::fs::create_dir_all(&logs_dir).ok();
    std::fs::create_dir_all(&output_dir).ok();

    if let Err(e) = logging::init_logging(&logs_dir) {
        eprintln!("初始化日志失败: {}", e);
    }

    tracing::info!("Penguin Downloader TUI 启动");

    let core = PenguinCore::new();
    tracing::info!("加载插件目录: {:?}", plugins_dir);
    if let Err(e) = core.load_plugins_from_dir(&plugins_dir).await {
        tracing::error!("加载插件失败: {}", e);
    }
    tracing::info!("已加载音源: {:?}", core.list_provider_names());

    // 扫描 plugins 目录下的文件并逐个尝试加载
    if let Ok(entries) = std::fs::read_dir(&plugins_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "dll" || ext == "so" || ext == "dylib" {
                        tracing::info!("尝试加载插件文件: {:?}", path);
                        if let Err(e) = core.load_plugin_from_file(&path).await {
                            tracing::warn!("加载插件文件失败 {:?}: {}", path, e);
                        }
                    }
                }
            }
        }
    }

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, crossterm::event::EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    while crossterm::event::poll(std::time::Duration::from_millis(0))? {
        let _ = crossterm::event::read()?;
    }

    let mut app = App::new(core, plugins_dir, credential_path, output_dir, config_path);
    if let Err(e) = app::run_app(&mut terminal, &mut app).await {
        tracing::error!("应用运行错误: {}", e);
    }

    app.save_credentials();
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    tracing::info!("Penguin Downloader TUI 退出");
    Ok(())
}
