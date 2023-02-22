use crate::{discord::Client, time};
use _tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Corner, Layout, Rect},
    style::{Color, Style},
    text::{Span, Spans, Text},
    widgets::{List, ListItem, Paragraph},
    Terminal,
};
use crossterm::{
    event::{self, Event, EventStream, KeyCode, KeyEvent, KeyModifiers},
    execute, terminal,
};
use futures_util::{FutureExt, StreamExt};
use std::collections::VecDeque;
use std::{borrow::Cow, future::IntoFuture, io, mem};
use twilight_http::{response::ResponseFuture, Response};
use twilight_model::{channel::Message, id::Id};

pub struct Application<'a> {
    create_message_queue: CreateMessageQueue,
    channel_query: Option<String>,
    damaged: bool,
    discord: Client,
    event_stream: EventStream,
    message: String,
    terminal: Terminal<CrosstermBackend<io::StdoutLock<'a>>>,
}

impl<'a> Application<'a> {
    /// Construct a new aplication.
    pub fn new() -> io::Result<Self> {
        let stdout = io::stdout().lock();
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        // TODO: login screen.
        let token = std::env::var("DISCORD_TOKEN").unwrap();

        let mut app = Self {
            create_message_queue: CreateMessageQueue::default(),
            channel_query: None,
            damaged: true,
            discord: Client::new(token),
            event_stream: EventStream::new(),
            message: String::new(),
            terminal,
        };

        app.enable_application_mode()?;

        Ok(app)
    }

    /// Enable application mode.
    pub fn enable_application_mode(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()?;

        execute!(
            self.terminal.backend_mut(),
            terminal::EnterAlternateScreen,
            event::EnableBracketedPaste,
            event::EnableFocusChange,
            event::EnableMouseCapture,
        )?;

        Ok(())
    }

    /// Disable application mode.
    pub fn disable_application_mode(&mut self) -> io::Result<()> {
        execute!(
            self.terminal.backend_mut(),
            event::DisableBracketedPaste,
            event::DisableFocusChange,
            event::DisableMouseCapture,
            terminal::LeaveAlternateScreen,
        )?;

        terminal::disable_raw_mode()?;

        Ok(())
    }

    /// Mark that the UI requires updating.
    pub fn damage(&mut self) {
        self.damaged = true;
    }

    /// Return whether the UI required updating.
    pub fn take_damage(&mut self) -> bool {
        mem::take(&mut self.damaged)
    }

    /// Main event loop.
    pub async fn run(&mut self) -> io::Result<()> {
        loop {
            if self.discord.take_message_cache_damage() {
                self.damage();
            }

            if self.take_damage() {
                self.terminal.draw(|frame| {
                    let size = frame.size();

                    let layout = Layout::default()
                        .constraints([
                            Constraint::Min(0),
                            Constraint::Length(1),
                            Constraint::Length(1),
                        ])
                        .split(size);

                    let list = message_list(&self.discord, layout[0]);
                    let text = if self.message.is_empty() {
                        let style = Style::default().fg(Color::Gray);

                        Paragraph::new(Text::styled("message", style))
                    } else {
                        Paragraph::new(self.message.as_str())
                    };

                    /*let list = self
                        .discord
                        .search_channel(&self.message)
                        .into_iter()
                        .map(|(name, id)| ListItem::new(format!("{name} {id}")))
                        .collect::<Vec<_>>();

                    let list = List::new(list);*/

                    frame.render_widget(list, layout[0]);
                    frame.render_widget(text, layout[2]);
                    frame.set_cursor(layout[2].x + self.message.len() as u16, layout[2].y);
                })?;
            }

            let discord_event = self.discord.next_event();
            let input_event = self.event_stream.next().fuse();
            let create_message_queue = &mut self.create_message_queue;

            tokio::select! {
                _result = create_message_queue => {},
                _maybe_event = discord_event => {}
                maybe_event = input_event => {
                    match maybe_event {
                        Some(Ok(event)) => {
                            if !self.process_event(event).await {
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

    /// Process an input event.
    pub async fn process_event(&mut self, event: Event) -> bool {
        // Refer to https://github.com/crossterm-rs/crossterm/issues/685 for what we can do here.
        match event {
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => match code {
                KeyCode::Esc => return false,
                KeyCode::Enter
                    if !(modifiers.contains(KeyModifiers::CONTROL)
                        || modifiers.contains(KeyModifiers::SHIFT)) =>
                {
                    if let Some(channel) = self.message.strip_prefix("/channel") {
                        let channel = channel.trim();

                        if let Some(channel) = channel.parse().ok().and_then(Id::new_checked) {
                            self.discord.current_channel = Some(channel);
                        }
                    } else if let Some(channel_id) = self.discord.current_channel {
                        let future = self
                            .discord
                            .rest
                            .create_message(channel_id)
                            .content(&self.message)
                            .unwrap()
                            .into_future();

                        self.create_message_queue.push(future);
                    }

                    self.message.clear();
                    self.damage();
                }
                KeyCode::Char(character) => {
                    self.message.push(character);
                    self.damage();
                }
                KeyCode::Backspace => {
                    self.message.pop();
                    self.damage();
                }
                _ => {}
            },
            Event::Paste(text) => {
                self.message.push_str(&text);
                self.damage();
            }
            Event::Resize(_width, _height) => {
                self.damage();
            }
            _ => {}
        }

        true
    }
}

impl<'a> Drop for Application<'a> {
    fn drop(&mut self) {
        let _ = self.disable_application_mode();
    }
}

/// Run the TUI version of strife.
pub async fn run() {
    let mut app = Application::new().unwrap();

    app.run().await.unwrap();
}

/// Render the entire message list.
pub fn message_list(discord: &Client, size: Rect) -> List<'static> {
    let Some(current_channel) = discord.current_channel else {
        return List::new([]);
    };

    let Some(message_ids) = discord
        .cache
        .channel_messages(current_channel) else {
        return List::new([]);
    };

    let (mut items, _last_id) = message_ids.iter().rev().fold(
        (Vec::new(), None),
        |(mut items, mut last_id), message_id| {
            let message = discord.cache.message(*message_id).unwrap();
            let author_id = message.author();
            let user = discord.cache.user(author_id).unwrap();
            let timestamp_style = Style::default().fg(Color::Gray);

            let show_header = !last_id
                .replace(author_id)
                .map(|old_author_id| old_author_id == author_id)
                .unwrap_or_default();

            if show_header {
                let mut header = vec![Span::raw(user.name.clone()), Span::raw(" ")];
                let timestamp = time::display_timestamp(message.timestamp());

                header.push(Span::styled(timestamp, timestamp_style));
                items.push(ListItem::new(Spans::from(header)));
            }

            let content = textwrap::wrap(message.content(), size.width as usize);
            let (last, rest) = content.split_last().unwrap();
            let rest = rest.iter().map(map_cow_to_list);
            let last = last.to_string();
            let last = if let Some(timestamp) = message.edited_timestamp() {
                let edited_timestamp = time::display_timestamp(timestamp);

                ListItem::new(Spans::from(vec![
                    Span::raw(last),
                    Span::raw(" "),
                    Span::styled("(edited at: ", timestamp_style),
                    Span::styled(edited_timestamp, timestamp_style),
                    Span::styled(")", timestamp_style),
                ]))
            } else {
                ListItem::new(last)
            };

            items.extend(rest);
            items.push(last);

            (items, last_id)
        },
    );

    items.reverse();

    List::new(items).start_corner(Corner::BottomLeft)
}

/// Attempt to declutter above code.
#[allow(clippy::ptr_arg)]
fn map_cow_to_list(text: &Cow<'_, str>) -> ListItem<'static> {
    if text.is_empty() {
        ListItem::new(" ")
    } else {
        ListItem::new(text.to_string())
    }
}

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

#[derive(Default)]
pub struct CreateMessageQueue {
    queue: VecDeque<ResponseFuture<Message>>,
}

impl CreateMessageQueue {
    pub fn push(&mut self, future: ResponseFuture<Message>) {
        self.queue.push_back(future);
    }
}

impl Future for CreateMessageQueue {
    type Output = Result<Response<Message>, twilight_http::Error>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { Pin::get_unchecked_mut(self) };

        if let Some(future) = this.queue.front_mut() {
            let poll = unsafe { Pin::new_unchecked(future) }.poll(context);

            match poll {
                Poll::Ready(result) => {
                    this.queue.pop_front();

                    Poll::Ready(result)
                }
                Poll::Pending => Poll::Pending,
            }
        } else {
            Poll::Pending
        }
    }
}
