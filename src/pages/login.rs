use std::sync::Arc;
use std::time::Duration;

use crossterm::event::KeyCode;
use penguin_downloader::{
    provider::{
        CodeLoginCallback, LoginMethodType, MusicProvider, QrLoginCallback, QrLoginData,
    },
    PenguinCore,
};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use tracing::info;
use crate::credential::CredentialStore;
use crate::login_util;
use crate::qr_renderer;

pub enum LoginPageAction {
    Back,
}

#[derive(Clone, Copy)]
pub enum AccountField {
    Username,
    Password,
}

#[derive(Clone, Copy)]
pub enum CodeField {
    Account,
    Code,
}

pub enum LoginState {
    Idle,
    QrWaiting,
    QrDisplay(String),
    AccountInput {
        username: String,
        password: String,
        focus: AccountField,
    },
    CodeInput {
        account: String,
        code: String,
        focus: CodeField,
    },
    LoggingIn,
    Success,
    Failed(String),
}

pub struct PendingLogin {
    pub result: Arc<std::sync::Mutex<Option<penguin_downloader::Result<String>>>>,
    pub qr_data: Option<Arc<std::sync::Mutex<Option<QrLoginData>>>>,
    pub provider_name: String,
}

impl PendingLogin {
    pub fn start_qr(provider: Arc<dyn MusicProvider>, method_index: usize) -> Option<Self> {
        let all = provider.list_login_methods();
        let method = all.into_iter().nth(method_index)?;
        let provider_name = provider.info().id;

        let qr_data = Arc::new(std::sync::Mutex::new(None));
        let result: Arc<std::sync::Mutex<Option<penguin_downloader::Result<String>>>> =
            Arc::new(std::sync::Mutex::new(None));
        let qd = qr_data.clone();
        let res = result.clone();

        tokio::task::spawn_blocking(move || {
            let qd_cb = qd.clone();
            let cb = Box::new(QrBgCallback { data: qd_cb });
            let login_result = login_util::perform_qr_login(&*method, cb, Duration::from_secs(180));
            *res.lock().unwrap() = Some(login_result);
        });

        Some(Self { result, qr_data: Some(qr_data), provider_name })
    }

    pub fn start_account(provider: Arc<dyn MusicProvider>, method_index: usize, username: String, password: String) -> Option<Self> {
        let all = provider.list_login_methods();
        let method = all.into_iter().nth(method_index)?;
        let provider_name = provider.info().id;
        let result: Arc<std::sync::Mutex<Option<penguin_downloader::Result<String>>>> =
            Arc::new(std::sync::Mutex::new(None));
        let r = result.clone();

        tokio::task::spawn_blocking(move || {
            let res = login_util::perform_account_login(&*method, username, password, Duration::from_secs(120));
            *r.lock().unwrap() = Some(res);
        });

        Some(Self { result, qr_data: None, provider_name })
    }

    pub fn start_code(provider: Arc<dyn MusicProvider>, method_index: usize, account: String, code: String) -> Option<Self> {
        let all = provider.list_login_methods();
        let method = all.into_iter().nth(method_index)?;
        let provider_name = provider.info().id;
        let result: Arc<std::sync::Mutex<Option<penguin_downloader::Result<String>>>> =
            Arc::new(std::sync::Mutex::new(None));
        let r = result.clone();

        tokio::task::spawn_blocking(move || {
            let cb = Box::new(SimpleCodeCallback { code });
            let res = login_util::perform_code_login(&*method, account, cb, Duration::from_secs(120));
            *r.lock().unwrap() = Some(res);
        });

        Some(Self { result, qr_data: None, provider_name })
    }
}

struct QrBgCallback {
    data: Arc<std::sync::Mutex<Option<QrLoginData>>>,
}

impl QrLoginCallback for QrBgCallback {
    fn on_qr_data(&self, data: QrLoginData) {
        *self.data.lock().unwrap() = Some(data);
    }
    fn clone_box(&self) -> Box<dyn QrLoginCallback> {
        Box::new(Self { data: self.data.clone() })
    }
}

pub struct LoginPageState {
    pub providers: Vec<(String, Arc<dyn MusicProvider>)>,
    pub selected_index: usize,
    pub show_methods: bool,
    pub method_index: usize,
    pub methods: Vec<login_util::MethodInfo>,
    pub login_state: LoginState,
    pub message: String,
    pub pending: Option<PendingLogin>,
    pub show_logout_confirm: bool,
    pub logout_confirm_yes: bool,
}

impl LoginPageState {
    pub fn new(providers: Vec<(String, Arc<dyn MusicProvider>)>, credential_store: &CredentialStore) -> Self {
        let mut s = Self {
            providers,
            selected_index: 0,
            show_methods: false,
            method_index: 0,
            methods: Vec::new(),
            login_state: LoginState::Idle,
            message: String::new(),
            pending: None,
            show_logout_confirm: false,
            logout_confirm_yes: false,
        };
        if let Some((name, p)) = s.providers.first() {
            if !credential_store.has_credential(name) {
                s.show_methods = true;
                s.methods = login_util::list_method_info(&p.list_login_methods());
            }
        }
        s
    }

    pub fn auto_show_methods(&mut self, credential_store: &CredentialStore) {
        if let Some((name, p)) = self.providers.get(self.selected_index) {
            if !credential_store.has_credential(name) {
                self.show_methods = true;
                self.methods = login_util::list_method_info(&p.list_login_methods());
            }
        }
    }
}

pub fn render_login_page(frame: &mut Frame, state: &LoginPageState, credential: &CredentialStore) {
    let area = frame.area();
    if area.width < 20 || area.height < 10 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(3), Constraint::Min(5), Constraint::Length(3)])
        .split(area);

    frame.render_widget(
        Paragraph::new("\u{767b}\u{5f55}\u{7ba1}\u{7406}")
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        chunks[0],
    );

    let inner = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .margin(1)
        .split(chunks[1]);

    let list_items: Vec<Line> = state.providers.iter().enumerate().map(|(i, (name, provider))| {
        let sel = i == state.selected_index;
        let has = credential.has_credential(name);
        let s = if has { "\u{2713}" } else { "\u{2717}" };
        let style = if sel { Style::default().fg(Color::Black).bg(Color::Cyan) } else { Style::default() };
        Line::from(Span::styled(format!("{} {}  {}", s, name, provider.name()), style))
    }).collect();
    frame.render_widget(
        Paragraph::new(list_items).block(Block::default().title("\u{97f3}\u{6e90}\u{5217}\u{8868}").borders(Borders::ALL)),
        inner[0],
    );

    let details = if let Some((name, provider)) = state.providers.get(state.selected_index) {
        let has_cred = credential.has_credential(name);
        let mut lines = vec![
            Line::from(vec![Span::styled("\u{97f3}\u{6e90}\u{4fe1}\u{606f}", Style::default().add_modifier(Modifier::BOLD).fg(Color::Yellow))]),
            Line::from(""),
            Line::from(format!("\u{540d}\u{79f0}: {}", provider.name())),
            Line::from(format!("ID: {}", name)),
            Line::from(""),
            Line::from(vec![Span::raw("\u{51ed}\u{8bc1}: "), Span::styled(if has_cred { "\u{5df2}\u{4fdd}\u{5b58}" } else { "\u{65e0}" }, Style::default().fg(if has_cred { Color::Green } else { Color::Red }))]),
            Line::from(""),
        ];
        if let LoginState::Failed(ref msg) = state.login_state {
            lines.push(Line::from(vec![Span::styled(format!("\u{9519}\u{8bef}: {}", msg), Style::default().fg(Color::Red))]));
            lines.push(Line::from(""));
        }
        if state.show_methods {
            lines.push(Line::from("\u{9009}\u{62e9}\u{767b}\u{5f55}\u{65b9}\u{5f0f}:"));
            for (i, m) in state.methods.iter().enumerate() {
                let sel = i == state.method_index;
                let prefix = if sel { " > " } else { "   " };
                let mt = match m.method_type {
                    LoginMethodType::QR => "\u{4e8c}\u{7ef4}\u{7801}",
                    LoginMethodType::URL => "\u{6253}\u{5f00}\u{7f51}\u{5740}",
                    LoginMethodType::Account => "\u{8d26}\u{53f7}\u{5bc6}\u{7801}",
                    LoginMethodType::Code => "\u{9a8c}\u{8bc1}\u{7801}",
                    LoginMethodType::None => "\u{65e0}",
                };
                lines.push(Line::from(Span::styled(format!("{}{} ({})", prefix, m.name, mt), if sel { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::White) })));
            }
            lines.push(Line::from(""));
        }
        let mut action = vec![];
        if has_cred {
            action.push(Span::styled("X", Style::default().fg(Color::Yellow)));
            action.push(Span::raw(" \u{6e05}\u{9664}\u{51ed}\u{8bc1}  "));
        }
        action.push(Span::styled("Esc", Style::default().fg(Color::Yellow)));
        action.push(Span::raw(" \u{8fd4}\u{56de}"));
        lines.push(Line::from(action));
        lines
    } else {
        vec![Line::from("\u{65e0}\u{53ef}\u{7528}\u{7684}\u{97f3}\u{6e90}")]
    };

    frame.render_widget(
        Paragraph::new(details).block(Block::default().title("\u{64cd}\u{4f5c}").borders(Borders::ALL)).wrap(Wrap { trim: true }),
        inner[1],
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("\u{2191}\u{2193}", Style::default().fg(Color::Yellow)),
            Span::raw(" \u{5207}\u{6362}\u{97f3}\u{6e90}  "),
            Span::styled("Tab", Style::default().fg(Color::Yellow)),
            Span::raw(" \u{5207}\u{6362}\u{65b9}\u{5f0f}  "),
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::raw(" \u{767b}\u{5f55}  "),
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::raw(" \u{8fd4}\u{56de}"),
        ]))
        .block(Block::default().borders(Borders::ALL)),
        chunks[2],
    );

    match &state.login_state {
        LoginState::QrDisplay(text) => render_qr_dialog(frame, area, text),
        LoginState::AccountInput { username, password, focus } => render_account_dialog(frame, area, username, password, *focus),
        LoginState::CodeInput { account, code, focus } => render_code_dialog(frame, area, account, code, *focus),
        LoginState::LoggingIn => render_loading_dialog(frame, area, "\u{767b}\u{5f55}\u{4e2d}..."),
        LoginState::Success => render_loading_dialog(frame, area, "\u{767b}\u{5f55}\u{6210}\u{529f}\u{ff0c}\u{51ed}\u{8bc1}\u{5df2}\u{4fdd}\u{5b58}"),
        _ => {}
    }

    if state.show_logout_confirm {
        render_logout_confirm(frame, area, state);
    }
}

fn render_logout_confirm(frame: &mut Frame, area: Rect, state: &LoginPageState) {
    let w = (area.width * 50 / 100).min(46).max(30);
    let h = 7u16;
    let x = area.x + (area.width - w) / 2;
    let y = area.y + (area.height - h) / 2;
    let d = Rect::new(x, y, w, h);
    frame.render_widget(Clear, d);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![Span::styled("\u{786e}\u{8ba4}\u{6e05}\u{9664}\u{6b64}\u{97f3}\u{6e90}\u{7684}\u{767b}\u{5f55}\u{51ed}\u{8bc1}?", Style::default().fg(Color::Yellow))]),
            Line::from(""),
            Line::from(vec![
                Span::styled(if state.logout_confirm_yes { " > \u{662f} " } else { "   \u{662f} " }, Style::default().fg(if state.logout_confirm_yes { Color::Green } else { Color::Gray })),
                Span::raw("  "),
                Span::styled(if !state.logout_confirm_yes { " > \u{5426} " } else { "   \u{5426} " }, Style::default().fg(if !state.logout_confirm_yes { Color::Red } else { Color::Gray })),
            ]),
            Line::from(""),
        ])
        .block(Block::default().borders(Borders::ALL).title("\u{786e}\u{8ba4}"))
        .alignment(Alignment::Center),
        d,
    );
}

fn centered_rect(px: u16, py: u16, r: Rect) -> Rect {
    let v = Layout::default().direction(Direction::Vertical)
        .constraints([Constraint::Percentage((100 - py) / 2), Constraint::Percentage(py), Constraint::Percentage((100 - py) / 2)]).split(r);
    Layout::default().direction(Direction::Horizontal)
        .constraints([Constraint::Percentage((100 - px) / 2), Constraint::Percentage(px), Constraint::Percentage((100 - px) / 2)]).split(v[1])[1]
}

fn render_qr_dialog(frame: &mut Frame, area: Rect, text: &str) {
    let line_count = text.lines().count() as u16;
    let h = (line_count + 4).min(area.height.saturating_sub(2)).max(10);
    let w = text.lines().next().map(|l| l.chars().count() as u16).unwrap_or(70) + 4;
    let w = w.min(area.width.saturating_sub(2)).max(30);
    let x = area.x + (area.width - w) / 2;
    let y = area.y + (area.height - h) / 2;
    let d = Rect::new(x, y, w, h);
    frame.render_widget(Clear, d);
    let mut lines = vec![
        Line::from(vec![Span::styled("\u{626b}\u{63cf}\u{4e8c}\u{7ef4}\u{7801}\u{767b}\u{5f55}", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))]),
        Line::from(""),
    ];
    for l in text.lines() { lines.push(Line::from(l.to_string())); }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled("\u{6309} ESC \u{53d6}\u{6d88}", Style::default().fg(Color::Gray))]));
    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("\u{4e8c}\u{7ef4}\u{7801}\u{767b}\u{5f55}"))
            .alignment(Alignment::Center).wrap(Wrap { trim: false }), d);
}

fn render_account_dialog(frame: &mut Frame, area: Rect, u: &str, p: &str, focus: AccountField) {
    let d = centered_rect(50, 30, area);
    frame.render_widget(Clear, d);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![Span::styled("\u{8d26}\u{53f7}\u{5bc6}\u{7801}\u{767b}\u{5f55}", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))]),
            Line::from(""),
            Line::from(vec![Span::styled(if matches!(focus, AccountField::Username) { "> " } else { "  " }, Style::default().fg(Color::Yellow)), Span::raw("\u{8d26}\u{53f7}: "), Span::raw(u.to_string())]),
            Line::from(""),
            Line::from(vec![Span::styled(if matches!(focus, AccountField::Password) { "> " } else { "  " }, Style::default().fg(Color::Yellow)), Span::raw("\u{5bc6}\u{7801}: "), Span::raw("*".repeat(p.len()))]),
            Line::from(""),
            Line::from(vec![Span::styled("Tab", Style::default().fg(Color::Yellow)), Span::raw(" \u{5207}\u{6362}  "), Span::styled("Enter", Style::default().fg(Color::Yellow)), Span::raw(" \u{767b}\u{5f55}  "), Span::styled("Esc", Style::default().fg(Color::Yellow)), Span::raw(" \u{53d6}\u{6d88}")]),
        ]).block(Block::default().borders(Borders::ALL)).alignment(Alignment::Center), d);
}

fn render_code_dialog(frame: &mut Frame, area: Rect, a: &str, c: &str, focus: CodeField) {
    let d = centered_rect(50, 30, area);
    frame.render_widget(Clear, d);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![Span::styled("\u{9a8c}\u{8bc1}\u{7801}\u{767b}\u{5f55}", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))]),
            Line::from(""),
            Line::from(vec![Span::styled(if matches!(focus, CodeField::Account) { "> " } else { "  " }, Style::default().fg(Color::Yellow)), Span::raw("\u{8d26}\u{53f7}: "), Span::raw(a.to_string())]),
            Line::from(""),
            Line::from(vec![Span::styled(if matches!(focus, CodeField::Code) { "> " } else { "  " }, Style::default().fg(Color::Yellow)), Span::raw("\u{9a8c}\u{8bc1}\u{7801}: "), Span::raw(c.to_string())]),
            Line::from(""),
            Line::from(vec![Span::styled("Tab", Style::default().fg(Color::Yellow)), Span::raw(" \u{5207}\u{6362}  "), Span::styled("Enter", Style::default().fg(Color::Yellow)), Span::raw(" \u{63d0}\u{4ea4}  "), Span::styled("Esc", Style::default().fg(Color::Yellow)), Span::raw(" \u{53d6}\u{6d88}")]),
        ]).block(Block::default().borders(Borders::ALL)).alignment(Alignment::Center), d);
}

fn render_loading_dialog(frame: &mut Frame, area: Rect, msg: &str) {
    let w = (area.width * 40 / 100).min(40).max(24);
    let h = 5u16;
    let x = area.x + (area.width - w) / 2;
    let y = area.y + (area.height - h) / 2;
    let d = Rect::new(x, y, w, h);
    frame.render_widget(Clear, d);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(msg, Style::default().fg(Color::Green))))
            .block(Block::default().borders(Borders::ALL)).alignment(Alignment::Center), d);
}

pub fn poll_bg_login(state: &mut LoginPageState, store: &mut CredentialStore) -> bool {
    let has_pending = state.pending.is_some();
    if !has_pending { return false; }

    let qr_data_option = state.pending.as_ref().and_then(|p| p.qr_data.clone());
    let result_arc = state.pending.as_ref().map(|p| p.result.clone());

    if let LoginState::QrWaiting = state.login_state {
        if let Some(ref qd) = qr_data_option {
            if let Ok(mut data) = qd.lock() {
                if let Some(qr_data) = data.take() {
                    let text = qr_renderer::render_to_string(&qr_data);
                    state.login_state = LoginState::QrDisplay(text);
                }
            }
        }
    }

    if let Some(ref res_arc) = result_arc {
        if let Ok(mut res) = res_arc.lock() {
            if let Some(result) = res.take() {
                let pname = state.pending.as_ref().map(|p| p.provider_name.clone()).unwrap_or_default();
                state.pending = None;
                match result {
                    Ok(credential) => {
                        store.set_credential(pname, credential);
                        state.login_state = LoginState::Success;
                        state.show_methods = false;
                        return true;
                    }
                    Err(e) => { state.login_state = LoginState::Failed(e.to_string()); }
                }
            }
        }
    }
    false
}

pub async fn handle_login_input(
    state: &mut LoginPageState,
    key: KeyCode,
    credential_store: &mut CredentialStore,
    _core: &PenguinCore,
) -> Option<LoginPageAction> {
    if state.show_logout_confirm {
        match key {
            KeyCode::Left | KeyCode::Right => { state.logout_confirm_yes = !state.logout_confirm_yes; }
            KeyCode::Enter => {
                state.show_logout_confirm = false;
                if state.logout_confirm_yes {
                    let name = &state.providers[state.selected_index].0;
                    credential_store.remove_credential(name);
                    state.message = format!("\u{5df2}\u{6e05}\u{9664} {} \u{7684}\u{51ed}\u{8bc1}", name);
                }
            }
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => { state.show_logout_confirm = false; }
            _ => {}
        }
        return None;
    }

    match &state.login_state {
        LoginState::QrDisplay(_) | LoginState::QrWaiting => {
            if key == KeyCode::Esc { state.pending = None; state.login_state = LoginState::Idle; }
            return None;
        }
        LoginState::LoggingIn => return None,
        LoginState::Success | LoginState::Failed(_) => {
            if key == KeyCode::Esc || key == KeyCode::Enter { state.login_state = LoginState::Idle; }
            return None;
        }
        _ => {}
    }

    if let LoginState::AccountInput { .. } = &state.login_state {
        handle_account_input(state, key, credential_store);
        return None;
    }
    if let LoginState::CodeInput { .. } = &state.login_state {
        handle_code_input(state, key, credential_store);
        return None;
    }

    // Provider navigation: Up/Down
    if key == KeyCode::Up || key == KeyCode::Down {
        if key == KeyCode::Up {
            state.selected_index = state.selected_index.saturating_sub(1);
        } else {
            if state.selected_index + 1 < state.providers.len() { state.selected_index += 1; }
        }
        state.method_index = 0;
        if let Some((name, p)) = state.providers.get(state.selected_index) {
            if !credential_store.has_credential(name) {
                state.show_methods = true;
                state.methods = login_util::list_method_info(&p.list_login_methods());
            } else {
                state.show_methods = false;
            }
        }
        return None;
    }

    // Esc: always back to home
    if key == KeyCode::Esc {
        state.pending = None;
        return Some(LoginPageAction::Back);
    }

    // Tab: switch methods
    if state.show_methods && (key == KeyCode::Tab || key == KeyCode::BackTab) {
        if key == KeyCode::Tab {
            if state.method_index + 1 < state.methods.len() { state.method_index += 1; } else { state.method_index = 0; }
        } else {
            if state.method_index > 0 { state.method_index -= 1; } else { state.method_index = state.methods.len().saturating_sub(1); }
        }
        return None;
    }

    // Enter: start login with selected method
    if key == KeyCode::Enter && state.show_methods {
        if let Some((_, ref provider)) = state.providers.get(state.selected_index) {
            let all = provider.list_login_methods();
            if let Some(method) = all.get(state.method_index) {
                match method.method_type() {
                    LoginMethodType::QR => {
                        if let Some(p) = PendingLogin::start_qr(provider.clone(), state.method_index) {
                            state.pending = Some(p);
                            state.login_state = LoginState::QrWaiting;
                        } else { state.message = "\u{542f}\u{52a8}\u{767b}\u{5f55}\u{5931}\u{8d25}".to_string(); }
                    }
                    LoginMethodType::Account => {
                        state.login_state = LoginState::AccountInput {
                            username: String::new(), password: String::new(), focus: AccountField::Username,
                        };
                    }
                    LoginMethodType::Code => {
                        state.login_state = LoginState::CodeInput {
                            account: String::new(), code: String::new(), focus: CodeField::Account,
                        };
                    }
                    LoginMethodType::URL => {
                        state.login_state = LoginState::LoggingIn;
                        if let Some(p) = PendingLogin::start_qr(provider.clone(), state.method_index) {
                            state.pending = Some(p);
                            state.login_state = LoginState::QrWaiting;
                        } else { state.message = "\u{542f}\u{52a8}\u{767b}\u{5f55}\u{5931}\u{8d25}".to_string(); }
                    }
                    LoginMethodType::None => state.message = "\u{6b64}\u{97f3}\u{6e90}\u{65e0}\u{9700}\u{767b}\u{5f55}".to_string(),
                }
            }
        }
        return None;
    }

    // X: clear credential (with confirmation)
    if key == KeyCode::Char('x') || key == KeyCode::Char('X') {
        let name = &state.providers[state.selected_index].0;
        if credential_store.has_credential(name) {
            state.show_logout_confirm = true;
        }
        return None;
    }

    None
}

fn handle_account_input(state: &mut LoginPageState, key: KeyCode, _store: &mut CredentialStore) {
    if let LoginState::AccountInput { ref mut username, ref mut password, ref mut focus } = state.login_state {
        match key {
            KeyCode::Tab => *focus = match focus { AccountField::Username => AccountField::Password, AccountField::Password => AccountField::Username },
            KeyCode::Enter => {
                let u = username.clone(); let p = password.clone();
                if u.is_empty() || p.is_empty() { state.message = "\u{8bf7}\u{8f93}\u{5165}\u{8d26}\u{53f7}\u{548c}\u{5bc6}\u{7801}".to_string(); return; }
                state.login_state = LoginState::LoggingIn;
                if let Some(pending) = PendingLogin::start_account(state.providers[state.selected_index].1.clone(), state.method_index, u, p) {
                    state.pending = Some(pending);
                } else { state.login_state = LoginState::Failed("\u{6b64}\u{97f3}\u{6e90}\u{4e0d}\u{652f}\u{6301}\u{8d26}\u{53f7}\u{5bc6}\u{7801}\u{767b}\u{5f55}".to_string()); }
            }
            KeyCode::Esc => state.login_state = LoginState::Idle,
            KeyCode::Char(c) => match focus { AccountField::Username => username.push(c), AccountField::Password => password.push(c) },
            KeyCode::Backspace => match focus { AccountField::Username => { username.pop(); } AccountField::Password => { password.pop(); } },
            _ => {}
        }
    }
}

fn handle_code_input(state: &mut LoginPageState, key: KeyCode, _store: &mut CredentialStore) {
    if let LoginState::CodeInput { ref mut account, ref mut code, ref mut focus, .. } = state.login_state {
        match key {
            KeyCode::Tab => *focus = match focus { CodeField::Account => CodeField::Code, CodeField::Code => CodeField::Account },
            KeyCode::Enter => match focus {
                CodeField::Account => *focus = CodeField::Code,
                CodeField::Code => {
                    let a = account.clone(); let c = code.clone();
                    if a.is_empty() || c.is_empty() { state.message = "\u{8bf7}\u{586b}\u{5199}\u{5b8c}\u{6574}\u{4fe1}\u{606f}".to_string(); return; }
                    state.login_state = LoginState::LoggingIn;
                    if let Some(pending) = PendingLogin::start_code(state.providers[state.selected_index].1.clone(), state.method_index, a, c) {
                        state.pending = Some(pending);
                    } else { state.login_state = LoginState::Failed("\u{6b64}\u{97f3}\u{6e90}\u{4e0d}\u{652f}\u{6301}\u{9a8c}\u{8bc1}\u{7801}\u{767b}\u{5f55}".to_string()); }
                }
            },
            KeyCode::Esc => state.login_state = LoginState::Idle,
            KeyCode::Char(c) => match focus { CodeField::Account => account.push(c), CodeField::Code => code.push(c) },
            KeyCode::Backspace => match focus { CodeField::Account => { account.pop(); } CodeField::Code => { code.pop(); } },
            _ => {}
        }
    }
}

struct SimpleCodeCallback { code: String }
impl CodeLoginCallback for SimpleCodeCallback {
    fn request_code(&self, _url: Option<&str>) -> String { self.code.clone() }
    fn clone_box(&self) -> Box<dyn CodeLoginCallback> { Box::new(Self { code: self.code.clone() }) }
}
