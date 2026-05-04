use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LyricsConfig {
    #[serde(default = "default_lyrics_type")]
    pub r#type: String,
    #[serde(default)]
    pub translation: bool,
    #[serde(default)]
    pub roma: bool,
}

fn default_lyrics_type() -> String {
    "none".to_string()
}

impl Default for LyricsConfig {
    fn default() -> Self {
        Self {
            r#type: "none".to_string(),
            translation: false,
            roma: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_threads")]
    pub threads: i32,
    #[serde(default)]
    pub lyrics: LyricsConfig,
}

fn default_threads() -> i32 {
    1
}

impl Default for Config {
    fn default() -> Self {
        Self {
            threads: 1,
            lyrics: LyricsConfig::default(),
        }
    }
}

impl Config {
    pub fn load_or_default(path: &Path) -> Self {
        if path.exists() {
            match std::fs::read_to_string(path) {
                Ok(content) => match toml::from_str(&content) {
                    Ok(cfg) => {
                        let cfg = Self::validate(cfg);
                        return cfg;
                    }
                    Err(e) => {
                        tracing::warn!("配置文件解析失败: {}, 使用默认配置", e);
                    }
                },
                Err(e) => {
                    tracing::warn!("读取配置文件失败: {}, 使用默认配置", e);
                }
            }
        }
        let cfg = Config::default();
        let _ = cfg.save(path);
        cfg
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        tracing::info!("配置已保存到: {:?}", path);
        Ok(())
    }

    fn validate(mut cfg: Self) -> Self {
        cfg.threads = cfg.threads.clamp(1, 8);
        match cfg.lyrics.r#type.as_str() {
            "none" | "normal" | "verbatim" => {}
            _ => cfg.lyrics.r#type = "none".to_string(),
        }
        if cfg.lyrics.r#type == "none" {
            cfg.lyrics.translation = false;
            cfg.lyrics.roma = false;
        }
        cfg
    }

    pub fn toml_string(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!("# 下载线程数 (1-8)\nthreads = {}\n\n", self.threads));
        s.push_str("[lyrics]\n");
        s.push_str("# 下载歌词的类型: none=不下载, normal=普通歌词, verbatim=逐字歌词\n");
        s.push_str(&format!("type = \"{}\"\n", self.lyrics.r#type));
        s.push_str("# 是否下载翻译（仅 type 不为 none 时有效）\n");
        s.push_str(&format!("translation = {}\n", self.lyrics.translation));
        s.push_str("# 是否下载罗马音（仅 type 不为 none 时有效）\n");
        s.push_str(&format!("roma = {}\n", self.lyrics.roma));
        s
    }
}
