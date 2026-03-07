use std::{path::PathBuf, time::Duration};

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    prelude::*,
    text::Line,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use tokio::{fs, sync::mpsc};

use crate::{
    app::{self, AnalysisResult},
    browser,
};

#[derive(Debug)]
enum WorkerEvent {
    Log(String),
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
}

impl TuiState {
    fn push_log(&mut self, line: impl Into<String>) {
        self.logs.push(line.into());
        if self.logs.len() > 20 {
            let _ = self.logs.remove(0);
        }
    }

    fn pgn_preview(pgn: &str) -> String {
        pgn.lines().take(7).collect::<Vec<_>>().join("\n")
    }
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
                WorkerEvent::Log(line) => state.push_log(line),
                WorkerEvent::Done(result) => {
                    state.processing = false;
                    match result {
                        Ok(res) => {
                            state.final_url = Some(res.final_analysis_url());
                            state.pgn_preview = TuiState::pgn_preview(&res.pgn);
                            state.last_result = Some(res);
                        }
                        Err(err) => {
                            state.final_url = None;
                            state.push_log(format!("Failed: {err}"));
                        }
                    }
                }
            }
        }

        terminal.draw(|frame| render(frame, &state))?;

        if event::poll(Duration::from_millis(80))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Enter => {
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

                        let txc = tx.clone();
                        tokio::spawn(async move {
                            let tx_progress = txc.clone();
                            let tx_done = txc.clone();
                            let mut progress = move |line: &str| {
                                let _ = tx_progress.try_send(WorkerEvent::Log(line.to_string()));
                            };

                            let output = app::resolve_with_progress(&url, &mut progress).await;
                            let _ = match output {
                                Ok(v) => tx_done.send(WorkerEvent::Done(Ok(v))).await,
                                Err(err) => tx_done
                                    .send(WorkerEvent::Done(Err(err.to_string())))
                                    .await,
                            };
                        });
                    }
                    KeyCode::Char('c') => {
                        if let Some(res) = &state.last_result {
                            if let Err(err) = crate::clipboard::copy_to_clipboard(&res.pgn) {
                                state.push_log(format!("Clipboard copy failed: {err}"));
                            } else {
                                state.push_log("PGN copied");
                            }
                        }
                    }
                    KeyCode::Char('o') => {
                        if let Some(result) = &state.last_result {
                            let _ = browser::open_url(&result.final_analysis_url());
                        } else {
                            state.push_log("No final URL available to open.");
                        }
                    }
                    KeyCode::Char('p') => {
                        if let Some(res) = &state.last_result {
                            let path = PathBuf::from("c2l-last.pgn");
                            let path_display = path.display().to_string();
                            let pgn = res.pgn.clone();
                            tokio::spawn(async move {
                                let _ = fs::write(path.clone(), pgn).await;
                            });
                            state.push_log(format!("PGN saved: {}", path_display));
                        }
                    }
                    KeyCode::Backspace => {
                        state.input.pop();
                    }
                    KeyCode::Char(ch) => {
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
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Fill(1),
        ])
        .split(frame.area());

    let input = Paragraph::new(Line::from(format!("URL: {}", state.input))).block(
        Block::default()
            .borders(Borders::ALL)
            .title("URL Input (Enter=run, q=quit)"),
    );
    frame.render_widget(input, root[0]);

    let status_text = if state.processing {
        "Status: processing"
    } else {
        "Status: idle"
    };
    let mut status = format!("{}\nFinal URL: {}", status_text, state.final_url.clone().unwrap_or_else(|| "-".to_string()),);
    if let Some(last) = &state.last_result {
        status.push_str(&format!("\nGame ID: {}", last.game_id));
    }

    frame.render_widget(
        Paragraph::new(status).block(Block::default().borders(Borders::ALL).title("Status")),
        root[1],
    );

    let logs = state
        .logs
        .iter()
        .map(|line| ListItem::new(line.clone()))
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(logs).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Logs (c:copy, o:open, p:save)"),
        ),
        root[2],
    );

    frame.render_widget(
        Paragraph::new(state.pgn_preview.clone())
            .block(Block::default().borders(Borders::ALL).title("PGN Preview")),
        root[3],
    );
}
