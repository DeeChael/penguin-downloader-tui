use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use tracing::field::Visit;
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

const MAX_LOG_FILES: usize = 20;

struct CustomLogLayer {
    writer: Mutex<File>,
}

impl<S> Layer<S> for CustomLogLayer
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        let level = metadata.level();
        let target = metadata.target();
        let file = metadata.file().unwrap_or("unknown");
        let line = metadata.line().unwrap_or(0);

        let time = chrono::Local::now().format("%H:%M:%S");
        let module = simplify_target(target);
        let file_path = simplify_file_path(file);

        let mut log_line = format!("{} {} {} {}:{} - ", time, level, module, file_path, line);

        struct FieldVisitor {
            result: String,
        }

        impl Visit for FieldVisitor {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.result.push_str(&format!("{:?}", value));
                } else {
                    if !self.result.is_empty() && !self.result.ends_with(" - ") {
                        self.result.push_str(", ");
                    }
                    self.result.push_str(&format!("{}={:?}", field.name(), value));
                }
            }
        }

        let mut visitor = FieldVisitor { result: String::new() };
        event.record(&mut visitor);
        log_line.push_str(&visitor.result);
        log_line.push('\n');

        if let Ok(mut writer) = self.writer.lock() {
            let _ = writer.write_all(log_line.as_bytes());
        }

        if *level == tracing::Level::ERROR {
            eprintln!("{}", log_line.trim());
        }
    }
}

fn simplify_target(target: &str) -> String {
    if target.starts_with("penguin_downloader") || target.starts_with("penguin_downloader::core") {
        "[core]".to_string()
    } else if target.starts_with("penguin_downloader_tui") {
        "[tui]".to_string()
    } else if target.contains("provider") {
        "[provider]".to_string()
    } else {
        format!("[{}]", target.split("::").next().unwrap_or(target))
    }
}

fn simplify_file_path(file: &str) -> String {
    if let Some(idx) = file.find("/src/") {
        file[idx + 5..].to_string()
    } else if let Some(idx) = file.find("\\src\\") {
        file[idx + 5..].to_string()
    } else {
        file.to_string()
    }
}

fn cleanup_old_logs(logs_dir: &Path) -> anyhow::Result<()> {
    let mut log_files: Vec<_> = std::fs::read_dir(logs_dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            let path = entry.path();
            path.is_file()
                && path.extension().map(|e| e == "log").unwrap_or(false)
                && path.file_stem().map(|s| s.to_string_lossy().starts_with("penguin-downloader_")).unwrap_or(false)
        })
        .map(|entry| entry.path())
        .collect();

    log_files.sort_by(|a, b| {
        let a_time = std::fs::metadata(a).and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let b_time = std::fs::metadata(b).and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        a_time.cmp(&b_time)
    });

    if log_files.len() >= MAX_LOG_FILES {
        let files_to_remove = log_files.len() - MAX_LOG_FILES + 1;
        for file in log_files.iter().take(files_to_remove) {
            let _ = std::fs::remove_file(file);
        }
    }

    Ok(())
}

pub fn init_logging(logs_dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(logs_dir)?;
    cleanup_old_logs(logs_dir)?;

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let log_file = logs_dir.join(format!("penguin-downloader_{}.log", timestamp));
    let file = File::create(&log_file)?;

    let layer = CustomLogLayer { writer: Mutex::new(file) };

    tracing_subscriber::registry()
        .with(layer.with_filter(EnvFilter::new("debug")))
        .init();

    tracing::info!("日志系统初始化完成: {:?}", log_file);
    Ok(())
}
