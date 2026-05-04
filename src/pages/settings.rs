use std::path::PathBuf;

use crossterm::event::KeyCode;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::config::{Config, LyricsConfig};

pub enum SettingsAction {
    Back,
    Saved,
}

#[derive(Clone, Copy, PartialEq)]
pub enum SettingsFocus {
    Threads,
    LyricsType,
    Translation,
    Roma,
    Reset,
    Save,
    Back,
}

pub struct SettingsState {
    pub threads: i32,
    pub lyrics_type: String,
    pub translation: bool,
    pub roma: bool,
    pub focus: SettingsFocus,
    pub config_path: PathBuf,
    pub message: String,
}

impl SettingsState {
    pub fn new(config: &Config, config_path: PathBuf) -> Self {
        Self {
            threads: config.threads,
            lyrics_type: config.lyrics.r#type.clone(),
            translation: config.lyrics.translation,
            roma: config.lyrics.roma,
            focus: SettingsFocus::Threads,
            config_path,
            message: String::new(),
        }
    }

    pub fn to_config(&self) -> Config {
        Config {
            threads: self.threads,
            lyrics: LyricsConfig {
                r#type: self.lyrics_type.clone(),
                translation: self.translation,
                roma: self.roma,
            },
        }
    }

    pub fn reset(&mut self) {
        let def = Config::default();
        self.threads = def.threads;
        self.lyrics_type = def.lyrics.r#type;
        self.translation = def.lyrics.translation;
        self.roma = def.lyrics.roma;
    }

    fn focus_next(&mut self) {
        self.focus = match self.focus {
            SettingsFocus::Threads => SettingsFocus::LyricsType,
            SettingsFocus::LyricsType => SettingsFocus::Translation,
            SettingsFocus::Translation => SettingsFocus::Roma,
            SettingsFocus::Roma => SettingsFocus::Reset,
            SettingsFocus::Reset => SettingsFocus::Save,
            SettingsFocus::Save => SettingsFocus::Back,
            SettingsFocus::Back => SettingsFocus::Threads,
        };
    }

    fn focus_prev(&mut self) {
        self.focus = match self.focus {
            SettingsFocus::Threads => SettingsFocus::Back,
            SettingsFocus::LyricsType => SettingsFocus::Threads,
            SettingsFocus::Translation => SettingsFocus::LyricsType,
            SettingsFocus::Roma => SettingsFocus::Translation,
            SettingsFocus::Reset => SettingsFocus::Roma,
            SettingsFocus::Save => SettingsFocus::Reset,
            SettingsFocus::Back => SettingsFocus::Save,
        };
    }
}

pub fn render_settings(frame: &mut Frame, state: &SettingsState) {
    let area = frame.area();
    if area.width < 30 || area.height < 10 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1),  // 0: title
            Constraint::Length(3),  // 1: threads
            Constraint::Length(1),  // 2: lyrics header
            Constraint::Length(3),  // 3: lyrics type
            Constraint::Length(3),  // 4: translation (skip if type=none)
            Constraint::Length(3),  // 5: roma (skip if type=none)
            Constraint::Length(1),  // 6: reset button
            Constraint::Length(1),  // 7: save button
            Constraint::Length(1),  // 8: back button
            Constraint::Min(0),     // 9: message / spacer
        ])
        .split(area);

    // Title
    frame.render_widget(
        Paragraph::new("设置")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        chunks[0],
    );

    // Threads
    render_setting_item(
        frame,
        chunks[1],
        "下载线程数",
        &format!("{}", state.threads),
        state.focus == SettingsFocus::Threads,
    );

    // Lyrics section header
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "歌词设置",
            Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
        ))),
        chunks[2],
    );

    // Lyrics type
    let type_label = match state.lyrics_type.as_str() {
        "none" => "不下载",
        "normal" => "普通歌词",
        "verbatim" => "逐字歌词",
        _ => "不下载",
    };
    render_setting_item(
        frame,
        chunks[3],
        "下载歌词",
        type_label,
        state.focus == SettingsFocus::LyricsType,
    );

    // Translation
    render_setting_item(
        frame,
        chunks[4],
        "下载翻译",
        if state.translation { "是" } else { "否" },
        state.focus == SettingsFocus::Translation,
    );

    // Roma
    render_setting_item(
        frame,
        chunks[5],
        "下载罗马音",
        if state.roma { "是" } else { "否" },
        state.focus == SettingsFocus::Roma,
    );

    // Buttons
    let btn_items = [
        ("重置所有设置", SettingsFocus::Reset),
        ("保存并关闭", SettingsFocus::Save),
        ("返回", SettingsFocus::Back),
    ];

    for (i, (label, f)) in btn_items.iter().enumerate() {
        let sel = state.focus == *f;
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("{}{}", if sel { "> " } else { "  " }, label),
                if sel {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                },
            )))
            .alignment(Alignment::Center),
            chunks[6 + i],
        );
    }

    // Message
    if !state.message.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(&state.message, Style::default().fg(Color::Yellow))))
                .alignment(Alignment::Center),
            chunks[9],
        );
    }
}

fn render_setting_item(frame: &mut Frame, area: Rect, label: &str, value: &str, focused: bool) {
    let style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::White)
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{}: ", label),
                if focused {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::Gray)
                },
            ),
            Span::styled(value.to_string(), style.add_modifier(Modifier::BOLD)),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(if focused {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::DarkGray)
                }),
        ),
        area,
    );
}

pub async fn handle_settings_input(
    state: &mut SettingsState,
    key: KeyCode,
) -> Option<SettingsAction> {
    match key {
        KeyCode::Esc => return Some(SettingsAction::Back),
        KeyCode::Up => state.focus_prev(),
        KeyCode::Down => state.focus_next(),
        KeyCode::Enter => {
            match state.focus {
                SettingsFocus::Reset => {
                    state.reset();
                    state.message = "已重置为默认设置".to_string();
                }
                SettingsFocus::Save => {
                    let cfg = state.to_config();
                    match cfg.save(&state.config_path) {
                        Ok(_) => return Some(SettingsAction::Saved),
                        Err(e) => state.message = format!("保存失败: {}", e),
                    }
                }
                SettingsFocus::Back => return Some(SettingsAction::Back),
                _ => {}
            }
        }
        KeyCode::Left => {
            match state.focus {
                SettingsFocus::Threads => {
                    if state.threads > 1 {
                        state.threads -= 1;
                    }
                }
                SettingsFocus::LyricsType => {
                    state.lyrics_type = match state.lyrics_type.as_str() {
                        "none" => "none".to_string(),
                        "normal" => "none".to_string(),
                        "verbatim" => "normal".to_string(),
                        _ => "none".to_string(),
                    };
                    if state.lyrics_type == "none" {
                        state.translation = false;
                        state.roma = false;
                    }
                }
                SettingsFocus::Translation => {
                    state.translation = false;
                }
                SettingsFocus::Roma => {
                    state.roma = false;
                }
                _ => {}
            }
        }
        KeyCode::Right => {
            match state.focus {
                SettingsFocus::Threads => {
                    if state.threads < 8 {
                        state.threads += 1;
                    }
                }
                SettingsFocus::LyricsType => {
                    state.lyrics_type = match state.lyrics_type.as_str() {
                        "none" => "normal".to_string(),
                        "normal" => "verbatim".to_string(),
                        "verbatim" => "verbatim".to_string(),
                        _ => "normal".to_string(),
                    };
                }
                SettingsFocus::Translation => {
                    state.translation = true;
                }
                SettingsFocus::Roma => {
                    state.roma = true;
                }
                _ => {}
            }
        }
        _ => {}
    }
    None
}
