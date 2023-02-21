use _tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Corner, Layout},
    widgets::{List, ListItem, Paragraph},
    Terminal,
};
use crossterm::{
    event::{self, Event, EventStream, KeyCode, KeyEvent},
    execute, terminal,
};
use futures_util::{FutureExt, StreamExt};
use std::io;

pub struct Application<'a> {
    terminal: Terminal<CrosstermBackend<io::StdoutLock<'a>>>,
    message: String,
    event_stream: EventStream,
}

impl<'a> Application<'a> {
    pub fn new() -> io::Result<Self> {
        let stdout = io::stdout().lock();
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        let mut app = Self {
            terminal,
            message: String::new(),
            event_stream: EventStream::new(),
        };

        app.enable_raw_mode()?;

        Ok(app)
    }

    pub fn enable_raw_mode(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()?;

        execute!(
            self.terminal.backend_mut(),
            terminal::EnterAlternateScreen,
            event::EnableMouseCapture
        )?;

        Ok(())
    }

    pub fn disable_raw_mode(&mut self) -> io::Result<()> {
        execute!(
            self.terminal.backend_mut(),
            terminal::LeaveAlternateScreen,
            event::DisableMouseCapture
        )?;

        terminal::disable_raw_mode()?;

        Ok(())
    }

    pub async fn run(&mut self) -> io::Result<()> {
        loop {
            let messages = ["mari", "hi", "lucy", "how are you", "mari", "nvm, hbu"];
            let messages = messages
                .iter()
                .rev()
                .map(|content| ListItem::new(*content))
                .collect::<Vec<_>>();

            self.terminal.draw(|frame| {
                let size = frame.size();

                let layout = Layout::default()
                    .constraints([
                        Constraint::Min(0),
                        Constraint::Length(1),
                        Constraint::Length(1),
                    ])
                    .split(size);

                let list = List::new(messages).start_corner(Corner::BottomLeft);
                let text = if self.message.is_empty() {
                    Paragraph::new("enter message here")
                } else {
                    Paragraph::new(self.message.as_str())
                };

                frame.render_widget(list, layout[0]);
                frame.render_widget(text, layout[2]);
                frame.set_cursor(layout[2].x + self.message.len() as u16, layout[2].y);
            })?;

            let event = self.event_stream.next().fuse();

            tokio::select! {
                maybe_event = event => {
                    match maybe_event {
                        Some(Ok(event)) => {
                            if !self.process_event(event) {
                                break;
                            }
                        },
                        Some(Err(_error)) => {
                            break;
                        },
                        None => break,
                    }
                }
            };
        }

        Ok(())
    }

    fn process_event(&mut self, event: Event) -> bool {
        if event == Event::Key(KeyCode::Esc.into()) {
            return false;
        } else if let Event::Key(KeyEvent { code, .. }) = event {
            if let KeyCode::Char(character) = code {
                self.message.push(character);
            }
        }

        true
    }
}

impl<'a> Drop for Application<'a> {
    fn drop(&mut self) {
        let _ = self.disable_raw_mode();
    }
}

pub async fn run() {
    let mut app = Application::new().unwrap();

    app.run().await.unwrap();
}
