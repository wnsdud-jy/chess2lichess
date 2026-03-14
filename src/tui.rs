use std::{path::PathBuf, time::Duration};

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    prelude::*,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
};
use tokio::{fs, sync::mpsc};

use crate::{
    app::{self, AnalysisResult},
    browser,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpgradeChoice {
    UpgradeNow,
    SkipOnce,
    MuteForSevenDays,
}

#[derive(Debug)]
enum WorkerEvent {
    Done(Result<AnalysisResult, String>),
}

#[derive(Default)]
struct TuiState {
    input: String,
    logs: Vec<String>,
    pgn_preview: String,
    processing: bool,
    final_url: Option<String>,
    last_result: Option<AnalysisResult>,
    message: String,
    tick: u64,
}

impl TuiState {
    fn push_log(&mut self, line: impl Into<String>) {
        let line = line.into();
        self.message = line.clone();
        self.logs.push(line);
        if self.logs.len() > 20 {
            let _ = self.logs.remove(0);
        }
    }

    fn reset_for_next_input(&mut self) {
        self.input.clear();
    }

    fn pgn_preview(pgn: &str) -> String {
        pgn.lines().take(7).collect::<Vec<_>>().join("\n")
    }
}

fn style_palette() -> (Style, Style, Style, Style, Color) {
    let panel_bg = Color::Rgb(23, 26, 31);
    let panel = Style::default().fg(Color::White).bg(panel_bg);
    let muted = Style::default()
        .fg(Color::Rgb(174, 181, 190))
        .bg(panel_bg)
        .add_modifier(Modifier::DIM);
    let success = Style::default()
        .fg(Color::Rgb(118, 255, 150))
        .bg(panel_bg)
        .add_modifier(Modifier::BOLD);
    let warn = Style::default()
        .fg(Color::Rgb(255, 214, 102))
        .bg(panel_bg)
        .add_modifier(Modifier::BOLD);
    let accent = Color::Rgb(93, 192, 255);
    (panel, muted, success, warn, accent)
}

fn spinner(step: u64) -> &'static str {
    const FRAMES: [&str; 4] = ["|", "/", "-", "\\"];
    FRAMES[(step % FRAMES.len() as u64) as usize]
}

fn clamp_right(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_string()
    } else {
        value.chars().take(max).collect::<String>()
    }
}

pub fn prompt_npm_upgrade(update: &app::NpmUpdateInfo) -> Result<app::NpmUpdatePromptChoice> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;
    let mut selected = UpgradeChoice::UpgradeNow;

    let result = (|| -> Result<app::NpmUpdatePromptChoice> {
        loop {
            terminal.draw(|frame| render_upgrade_prompt(frame, update, selected))?;

            if !event::poll(Duration::from_millis(100))? {
                continue;
            }

            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Left | KeyCode::Up | KeyCode::Char('h') | KeyCode::Char('k') => {
                        selected = match selected {
                            UpgradeChoice::UpgradeNow => UpgradeChoice::MuteForSevenDays,
                            UpgradeChoice::SkipOnce => UpgradeChoice::UpgradeNow,
                            UpgradeChoice::MuteForSevenDays => UpgradeChoice::SkipOnce,
                        };
                    }
                    KeyCode::Right
                    | KeyCode::Down
                    | KeyCode::Tab
                    | KeyCode::Char('l')
                    | KeyCode::Char('j') => {
                        selected = match selected {
                            UpgradeChoice::UpgradeNow => UpgradeChoice::SkipOnce,
                            UpgradeChoice::SkipOnce => UpgradeChoice::MuteForSevenDays,
                            UpgradeChoice::MuteForSevenDays => UpgradeChoice::UpgradeNow,
                        };
                    }
                    KeyCode::Char('y') => return Ok(app::NpmUpdatePromptChoice::UpgradeNow),
                    KeyCode::Char('n') | KeyCode::Esc => {
                        return Ok(app::NpmUpdatePromptChoice::SkipOnce);
                    }
                    KeyCode::Char('m') => {
                        return Ok(app::NpmUpdatePromptChoice::MuteForSevenDays);
                    }
                    KeyCode::Enter => {
                        return Ok(match selected {
                            UpgradeChoice::UpgradeNow => app::NpmUpdatePromptChoice::UpgradeNow,
                            UpgradeChoice::SkipOnce => app::NpmUpdatePromptChoice::SkipOnce,
                            UpgradeChoice::MuteForSevenDays => {
                                app::NpmUpdatePromptChoice::MuteForSevenDays
                            }
                        });
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    })();

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn render_upgrade_prompt(frame: &mut Frame, update: &app::NpmUpdateInfo, selected: UpgradeChoice) {
    let (panel_style, muted_style, success_style, warn_style, accent) = style_palette();
    let popup = centered_rect(78, 15, frame.area());
    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(3),
        ])
        .margin(1)
        .split(popup);

    frame.render_widget(Clear, popup);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(panel_style)
            .style(panel_style)
            .title(Span::styled(
                " npm update available ",
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            )),
        popup,
    );

    frame.render_widget(
        Paragraph::new("A newer npm package version is available for c2l.")
            .style(panel_style)
            .wrap(Wrap { trim: true }),
        inner[0],
    );
    frame.render_widget(
        Paragraph::new(format!(
            "Current: {}    Latest: {}",
            update.current_version, update.latest_version
        ))
        .style(success_style)
        .wrap(Wrap { trim: true }),
        inner[1],
    );
    frame.render_widget(
        Paragraph::new(format!("Run: {}", app::npm_install_command_display(update)))
            .style(muted_style)
            .wrap(Wrap { trim: true }),
        inner[2],
    );
    frame.render_widget(
        Paragraph::new("Use ←/→ or y/n/m, then Enter.")
            .style(warn_style)
            .wrap(Wrap { trim: true }),
        inner[3],
    );

    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(inner[4]);

    let upgrade_style = if selected == UpgradeChoice::UpgradeNow {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Rgb(118, 255, 150))
            .add_modifier(Modifier::BOLD)
    } else {
        panel_style
    };
    let skip_style = if selected == UpgradeChoice::SkipOnce {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Rgb(255, 214, 102))
            .add_modifier(Modifier::BOLD)
    } else {
        panel_style
    };
    let mute_style = if selected == UpgradeChoice::MuteForSevenDays {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Rgb(255, 160, 122))
            .add_modifier(Modifier::BOLD)
    } else {
        panel_style
    };

    frame.render_widget(
        Paragraph::new("Upgrade now")
            .style(upgrade_style)
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(upgrade_style),
            ),
        buttons[0],
    );
    frame.render_widget(
        Paragraph::new("Skip once")
            .style(skip_style)
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(skip_style),
            ),
        buttons[1],
    );
    frame.render_widget(
        Paragraph::new("Mute 7 days")
            .style(mute_style)
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(mute_style),
            ),
        buttons[2],
    );
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(height.min(area.height.saturating_sub(2))),
            Constraint::Fill(1),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(width.min(area.width.saturating_sub(2))),
            Constraint::Fill(1),
        ])
        .split(vertical[1])[1]
}

pub async fn run_tui() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let (tx, mut rx) = mpsc::channel::<WorkerEvent>(32);
    let mut state = TuiState {
        ..TuiState::default()
    };

    loop {
        while let Ok(event) = rx.try_recv() {
            match event {
                WorkerEvent::Done(result) => {
                    state.processing = false;
                    match result {
                        Ok(res) => {
                            state.final_url = Some(res.final_analysis_url());
                            state.pgn_preview = TuiState::pgn_preview(&res.pgn);
                            state.last_result = Some(res);
                            state.push_log("Done. Enter next URL, or q to exit.");
                        }
                        Err(err) => {
                            state.final_url = None;
                            state.last_result = None;
                            state.push_log(format!("Failed: {err}"));
                        }
                    }
                    state.reset_for_next_input();
                }
            }
        }

        state.tick = state.tick.wrapping_add(1);
        terminal.draw(|frame| render(frame, &state))?;

        if event::poll(Duration::from_millis(80))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key {
                    KeyEvent {
                        code: KeyCode::Char('q'),
                        ..
                    } => break,
                    KeyEvent {
                        code: KeyCode::Esc, ..
                    } => break,
                    KeyEvent {
                        code: KeyCode::Char('c'),
                        modifiers,
                        ..
                    } if modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyEvent {
                        code: KeyCode::Enter,
                        ..
                    } => {
                        if state.processing {
                            continue;
                        }
                        let url = state.input.trim().to_string();
                        if url.is_empty() {
                            state.push_log("Please enter a URL.");
                            continue;
                        }
                        state.processing = true;
                        state.push_log(format!("Started processing: {url}"));
                        state.input.clear();

                        let txc = tx.clone();
                        tokio::spawn(async move {
                            let tx_done = txc.clone();
                            let options = app::RunOptions {
                                copy: false,
                                force_open: false,
                                no_open: true,
                                print_pgn: false,
                                save_pgn: None,
                                raw_url: false,
                                json_output: false,
                                csv_output: false,
                                quiet: false,
                                verbose: true,
                            };

                            let output = app::resolve_with_progress(&url, &options).await;
                            let _ = match output {
                                Ok(v) => tx_done.send(WorkerEvent::Done(Ok(v))).await,
                                Err(err) => {
                                    tx_done.send(WorkerEvent::Done(Err(err.to_string()))).await
                                }
                            };
                        });
                    }
                    KeyEvent {
                        code: KeyCode::Char('c'),
                        ..
                    } => {
                        if let Some(res) = &state.last_result {
                            if let Err(err) = crate::clipboard::copy_to_clipboard(&res.pgn) {
                                state.push_log(format!("Clipboard copy failed: {err}"));
                            } else {
                                state.push_log("PGN copied");
                            }
                        } else {
                            state.push_log("No PGN available yet.");
                        }
                    }
                    KeyEvent {
                        code: KeyCode::Char('o'),
                        ..
                    } => {
                        if let Some(result) = &state.last_result {
                            let url = result.final_analysis_url();
                            if let Err(err) = browser::open_url(&url) {
                                state.push_log(format!("Open failed: {err}"));
                            } else {
                                state.push_log(format!("Open requested: {}", url));
                            }
                        } else {
                            state.push_log("No final URL available to open.");
                        }
                    }
                    KeyEvent {
                        code: KeyCode::Char('p'),
                        ..
                    } => {
                        if let Some(res) = &state.last_result {
                            let path = PathBuf::from("c2l-last.pgn");
                            let path_display = path.display().to_string();
                            let pgn = res.pgn.clone();
                            tokio::spawn(async move {
                                let _ = fs::write(path.clone(), pgn).await;
                            });
                            state.push_log(format!("PGN saved: {path_display}"));
                        } else {
                            state.push_log("No PGN available to save.");
                        }
                    }
                    KeyEvent {
                        code: KeyCode::Backspace,
                        ..
                    } => {
                        state.input.pop();
                    }
                    KeyEvent {
                        code: KeyCode::Char(ch),
                        ..
                    } => {
                        state.input.push(ch);
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn render(frame: &mut Frame, state: &TuiState) {
    let (panel_style, muted_style, success_style, warn_style, accent) = style_palette();

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(frame.area());

    let header = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(panel_style)
        .title(Span::styled(
            " chess2lichess TUI ",
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(Color::Reset));

    let processing = if state.processing {
        format!("{} Processing", spinner(state.tick))
    } else {
        "✓ Idle".to_string()
    };

    let last_url = state
        .final_url
        .as_deref()
        .map(|url| clamp_right(url, 84))
        .unwrap_or_else(|| "-".to_string());

    let game_id = state
        .last_result
        .as_ref()
        .map(|result| result.game_id.as_str())
        .unwrap_or("-");
    let players = state
        .last_result
        .as_ref()
        .and_then(|result| result.metadata.players_label())
        .unwrap_or_else(|| "-".to_string());
    let summary = state
        .last_result
        .as_ref()
        .map(|result| {
            let bits = result.metadata.summary_bits();
            if bits.is_empty() {
                "-".to_string()
            } else {
                bits.join(" | ")
            }
        })
        .unwrap_or_else(|| "-".to_string());

    let status_lines = vec![
        Line::from(format!("Status: {processing}")),
        Line::from(format!("Final URL: {last_url}")),
        Line::from(format!("Game ID: {game_id}")),
        Line::from(format!("Players: {}", clamp_right(&players, 40))),
        Line::from(format!("Meta: {}", clamp_right(&summary, 40))),
    ];
    let status_block_lines = status_lines.clone();

    let info = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(Span::styled(
            " Status ",
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            " Enter next URL and press Enter again ",
            muted_style,
        ))
        .border_style(panel_style)
        .style(panel_style);

    let header_widget = Paragraph::new(status_lines)
        .block(header)
        .wrap(Wrap { trim: true });
    frame.render_widget(header_widget, root[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(root[1]);

    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(8),
            Constraint::Min(0),
        ])
        .split(body[0]);

    let input_panel = Paragraph::new(format!("URL: {}", state.input))
        .style(panel_style)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(panel_style)
                .title(Span::styled(
                    " URL Input ",
                    Style::default()
                        .fg(Color::Rgb(129, 212, 250))
                        .add_modifier(Modifier::BOLD),
                ))
                .title_bottom(Span::styled(
                    " Enter: run   c:copy   o:open   p:save   q:quit ",
                    muted_style,
                )),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(input_panel, left[0]);

    frame.render_widget(
        Paragraph::new(status_block_lines)
            .style(panel_style)
            .block(info)
            .wrap(Wrap { trim: true }),
        left[1],
    );

    let mut messages = state
        .logs
        .iter()
        .map(|line| ListItem::new(Span::styled(line.clone(), panel_style)))
        .collect::<Vec<_>>();
    if messages.is_empty() {
        messages.push(ListItem::new(Span::styled("No logs yet.", muted_style)));
    }

    frame.render_widget(
        List::new(messages)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(panel_style)
                    .title(Span::styled(" Logs ", success_style))
                    .title_bottom(Span::styled(format!(" {} ", state.message), warn_style)),
            )
            .style(panel_style),
        left[2],
    );

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(12)])
        .split(body[1]);

    let pgn = if state.pgn_preview.is_empty() {
        "No PGN yet.".to_string()
    } else {
        state.pgn_preview.clone()
    };

    frame.render_widget(
        Paragraph::new(pgn)
            .style(panel_style)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(panel_style)
                    .title(Span::styled(
                        " PGN Preview ",
                        Style::default()
                            .fg(Color::Rgb(255, 183, 197))
                            .add_modifier(Modifier::BOLD),
                    )),
            )
            .wrap(Wrap { trim: true }),
        right[1],
    );

    frame.render_widget(
        Paragraph::new(Span::styled(
            "Space left for next URL input | Terminal should support 24-bit color for best result",
            muted_style,
        ))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(panel_style)
                .title(Span::styled(
                    " Hints ",
                    Style::default().fg(accent).add_modifier(Modifier::BOLD),
                )),
        )
        .wrap(Wrap { trim: true }),
        right[0],
    );

    let footer_style = if state.processing {
        warn_style
    } else {
        muted_style
    };
    frame.render_widget(
        Paragraph::new(state.message.clone())
            .style(footer_style)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(panel_style)
                    .title(Span::styled(" Message ", Style::default().fg(accent))),
            ),
        root[2],
    );
}
