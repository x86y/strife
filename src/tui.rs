use self::terminal::{
    layout::{Constraint, Corner, Layout, Rect},
    style::{Color, Style},
    text::{Span, Spans, Text},
    widgets::{List, ListItem, Paragraph},
    Terminal,
};
use crate::{discord::Client, time};
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers};
use futures_util::{FutureExt, StreamExt};
use std::{borrow::Cow, future::IntoFuture, io};
use strife_discord::{
    cache::model::CachedMessage,
    model::{channel::Message, id::Id, user::User},
    ResponseQueue,
};

pub mod terminal;

pub struct App {
    create_message_queue: ResponseQueue<Message>,
    channel_query: Option<String>,
    discord: Client,
    event_stream: EventStream,
    message: String,
    terminal: Terminal,
}

impl App {
    /// Construct a new aplication.
    pub fn new() -> io::Result<Self> {
        // TODO: login screen.
        let token = std::env::var("DISCORD_TOKEN").unwrap();

        Ok(Self {
            create_message_queue: ResponseQueue::default(),
            channel_query: None,
            discord: Client::new(token),
            event_stream: EventStream::new(),
            message: String::new(),
            terminal: Terminal::new()?,
        })
    }

    /// Main event loop.
    pub async fn run(&mut self) -> io::Result<()> {
        loop {
            self.terminal.damage();
            self.terminal.render(|frame| {
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

            let discord_event = self.discord.next_event();
            let input_event = self.event_stream.next().fuse();
            let create_message_queue = &mut self.create_message_queue;

            tokio::select! {
                _result = create_message_queue => {},
                _maybe_event = discord_event => {
                    if self.discord.take_message_cache_damage() {
                        self.terminal.damage();
                    }
                }
                maybe_event = input_event => {
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

    /// Process an input event.
    pub fn process_event(&mut self, event: Event) -> bool {
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
                    self.terminal.damage();
                }
                KeyCode::Char(character) => {
                    self.message.push(character);
                    self.terminal.damage();
                }
                KeyCode::Backspace => {
                    self.message.pop();
                    self.terminal.damage();
                }
                _ => {}
            },
            Event::Paste(text) => {
                self.message.push_str(&text);
                self.terminal.damage();
            }
            Event::Resize(_width, _height) => {
                self.terminal.damage();
            }
            _ => {}
        }

        true
    }
}

/// Run the TUI version of strife.
pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new()?;

    app.run().await.unwrap();

    Ok(())
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

            let was_none = last_id.is_none();
            let show_header = !last_id
                .replace(author_id)
                .map(|old_author_id| old_author_id == author_id)
                .unwrap_or_default();

            if show_header {
                if !was_none {
                    items.push(ListItem::new(" "));
                }

                let mut header = vec![username(discord, &message, &user), Span::raw(" ")];
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

/// Render a username.
pub fn username(discord: &Client, message: &CachedMessage, user: &User) -> Span<'static> {
    let mut style = Style::default();

    if let Some(color) = user.accent_color.map(from_u32) {
        style = style.fg(color);
    }

    let Some(member) = message.member() else {
        return Span::styled(user.name.clone(), style);
    };

    let name = member.nick.clone().unwrap_or_else(|| user.name.clone());

    let mut roles = member
        .roles
        .iter()
        .flat_map(|role_id| discord.cache.role(*role_id))
        .filter(|role| role.color != 0)
        .collect::<Vec<_>>();

    roles.sort_unstable_by_key(|role| role.position);

    if let Some(role) = roles.last() {
        style = style.fg(from_u32(role.color));
    }

    Span::styled(name, style)
}

use palette::{rgb::channels, Pixel, Srgba};

/// Convert a [`u32`](u32) colour code to a [`Color`](Color).
pub fn from_u32(color: u32) -> Color {
    let rgba: Srgba<u8> = Srgba::from_u32::<channels::Argb>(color);
    let [r, g, b]: [u8; 3] = rgba.color.into_raw();

    Color::Rgb(r, g, b)
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
