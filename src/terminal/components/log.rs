use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::domain::service::Service;
use crate::terminal::app::{Actions, AppEvent};
use crate::usecases::services_manager::ServicesManager;

enum BorderColor {
    White,
    Orange,
}

impl BorderColor {
    fn to_color(&self) -> Color {
        match self {
            BorderColor::White => Color::White,
            BorderColor::Orange => Color::Rgb(255, 165, 0),
        }
    }
}

pub struct ServiceLog<'a> {
    log_paragraph: Option<Paragraph<'a>>,
    log_block: Option<Block<'a>>,
    border_color: BorderColor,
    service_name: String,
    scroll: u16,
    sender: Sender<AppEvent>,
    auto_refresh: Arc<Mutex<bool>>,
}

impl ServiceLog<'_> {
    pub fn new(sender: Sender<AppEvent>) -> Self {
        Self {
            log_paragraph: None,
            log_block: None,
            border_color: BorderColor::White,
            service_name: String::new(),
            scroll: 0,
            sender,
            auto_refresh: Arc::new(Mutex::new(false)),
        }
    }

    fn render_loading(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL);

        frame.render_widget(block.clone(), area);

        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Percentage(45),
                Constraint::Length(1),
                Constraint::Percentage(45),
            ])
            .split(area);

        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25),
                Constraint::Percentage(50),
                Constraint::Percentage(25),
            ])
            .split(vertical[1]);

        let loading = Paragraph::new("Loading...").alignment(Alignment::Center);

        frame.render_widget(loading, horizontal[1]);
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        if self.log_paragraph.is_none() || self.log_block.is_none() {
            self.render_loading(frame, area);
            return;
        }

        let log_block = self.log_block.clone().unwrap();
        let paragraph = self
            .log_paragraph
            .clone()
            .unwrap()
            .scroll((self.scroll, 0))
            .block(log_block);

        frame.render_widget(paragraph, area);
    }

    fn toogle_auto_refresh(&mut self) {
        let new_value = {
            if let Ok(auto) = self.auto_refresh.lock() {
                !*auto
            } else {
                return;
            }
        };

        self.set_auto_refresh(new_value);
    }

    fn set_auto_refresh(&mut self, value: bool) {
        self.border_color = if value {
            BorderColor::Orange
        } else {
            BorderColor::White
        };

        self.log_block = Some(
            Block::default()
                .title(format!(" {} logs (newest at the top) ", self.service_name))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(self.border_color.to_color()))
                .title_alignment(Alignment::Center),
        );

        if let Ok(mut auto) = self.auto_refresh.lock() {
            *auto = value;
        }
    }

    pub fn on_key_event(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Right => {
                self.reset();
                self.sender
                    .send(AppEvent::Action(Actions::GoDetails))
                    .unwrap();
            }
            KeyCode::Left => {
                self.reset();
                self.sender
                    .send(AppEvent::Action(Actions::GoDetails))
                    .unwrap();
            }
            KeyCode::Up => {
                self.scroll = self.scroll.saturating_sub(1);
            }
            KeyCode::Down => {
                self.scroll += 1;
            }
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.scroll += 10;
            }
            KeyCode::Char('a') => self.toogle_auto_refresh(),
            KeyCode::Char('q') => {
                self.reset();
                self.exit();
            }
            _ => {}
        }
    }

    pub fn shortcuts(&mut self) -> Vec<Line<'_>> {
        let is_refreshing = self.auto_refresh.lock().map(|r| *r).unwrap_or(false);
        let mut auto_refresh_label = "Enable auto-refresh";
        if is_refreshing {
            auto_refresh_label = "Disable auto-refresh";
        }

        let help_text = vec![
            Line::from(vec![Span::styled(
                "Actions",
                Style::default()
                    .fg(Color::LightMagenta)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(format!(
                "Scroll: ↑/↓ | Switch tabs: ←/→ | {}: a | Go back: q",
                auto_refresh_label
            )),
        ];

        help_text
    }

    pub fn start_auto_refresh(&mut self) {
        self.set_auto_refresh(true);
        self.auto_refresh_thread();
    }

    pub fn reset(&mut self) {
        self.set_auto_refresh(false);
        self.scroll = 0;
        self.log_paragraph = None;
    }

    fn exit(&self) {
        self.sender.send(AppEvent::Action(Actions::GoList)).unwrap();
    }

    pub fn auto_refresh_thread(&mut self) {
        let auto_refresh = Arc::clone(&self.auto_refresh);
        let sender = self.sender.clone();
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_millis(1000));
                if let Ok(is_active) = auto_refresh.lock() {
                    if *is_active {
                        sender.send(AppEvent::Action(Actions::RefreshLog)).unwrap();
                    } else {
                        break;
                    }
                }
            }
        });
    }

    pub fn fetch_log_and_dispatch(&mut self, service: Service) {
        let event_tx = self.sender.clone();
        thread::spawn(move || {
            if let Ok(log) = ServicesManager::get_log(&service) {
                event_tx
                    .send(AppEvent::Action(Actions::Updatelog((
                        service.name().to_string(),
                        log,
                    ))))
                    .expect("Failed to send Updatelog event");
            }
        });
    }

    pub fn update(&mut self, service_name: String, log: String) {
        self.service_name = service_name;
        self.log_paragraph =
            Some(Paragraph::new(self.reversed_log(log)).wrap(Wrap { trim: false }));
        self.log_block = Some(
            Block::default()
                .title(format!(" {} logs (newest at the top) ", self.service_name))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(self.border_color.to_color()))
                .title_alignment(Alignment::Center),
        );
    }

    pub fn reversed_log(&self, log: String) -> String {
        log.lines().rev().collect::<Vec<_>>().join("\n")
    }
}
