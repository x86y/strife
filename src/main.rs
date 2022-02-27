use crossterm::event::EventStream;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io;
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Corner, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans, Text},
    widgets::{Block, List, ListItem, Paragraph},
    Frame, Terminal,
};
use unicode_width::UnicodeWidthStr;

use discord_gateway_stream::{Gateway, GatewayEvent};
use futures_util::future::{BoxFuture, FutureExt, IntoStream};
use futures_util::stream;
use futures_util::stream::{Map, Select, Stream, StreamExt};
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{env, mem};
use twilight_gateway::shard::{Events, ShardStartError};
use twilight_gateway::Shard;
use twilight_model::gateway::payload::outgoing::identify::IdentifyProperties;
use twilight_model::gateway::payload::outgoing::update_presence::UpdatePresencePayload;
use twilight_model::gateway::presence::Status;
use twilight_model::gateway::Intents;
use twilight_model::id::{ChannelId, MessageId, UserId};

enum InputMode {
    Normal,
    Editing,
}

enum Kind {
    User,
    Channel,
}

use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Debug)]
struct Message {
    pub content: Box<str>,
    pub id: MessageId,
    pub author: Arc<User>,
}

#[derive(Clone, Debug)]
struct User {
    pub id: UserId,
    pub username: Box<str>,
}

#[derive(Clone, Debug)]
struct Channel {
    pub id: ChannelId,
    pub messages: BTreeMap<MessageId, Arc<RwLock<Message>>>,
    pub name: Box<str>,
}

use parking_lot::RwLock;

/// App holds the state of the application
struct App {
    /// Current value of the input box
    input: String,
    /// Current input mode
    input_mode: InputMode,
    /// History of recorded messages
    messages: Vec<String>,
    channels: Vec<(Kind, String)>,

    current_channel: Option<ChannelId>,
    channel_map: BTreeMap<ChannelId, Arc<RwLock<Channel>>>,
    message_map: BTreeMap<MessageId, Arc<RwLock<Message>>>,
    user_map: BTreeMap<UserId, Arc<User>>,
}

impl Default for App {
    fn default() -> App {
        App {
            input: String::new(),
            input_mode: InputMode::Editing,
            messages: vec![],
            channels: vec![],

            current_channel: None,
            channel_map: BTreeMap::new(),
            message_map: BTreeMap::new(),
            user_map: BTreeMap::new(),
        }
    }
}

type Error = Box<dyn std::error::Error + Send + Sync + 'static>;
type Result<T, E = Error> = std::result::Result<T, E>;

mod log;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    // Setup logging to `$CARGO_MANIFEST_DIR/log`.
    let logger = log::Logger::new()?;
    let (non_blocking, _guard) = tracing_appender::non_blocking(logger);

    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(non_blocking);

    tracing::subscriber::set_global_default(subscriber.finish())?;

    // setup terminal
    enable_raw_mode()?;

    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and run it
    let mut app = App::default();

    let token = env::var("DISCORD_TOKEN")?;
    let intents = Intents::GUILD_MESSAGES;

    // 115.4 - Beta on a S10 5G running Android 12.
    pub const ANDROID_CLIENT_VERSION: &str = "115.4 - Beta";
    pub const ANDROID_CLIENT_BUILD_NUMBER: &str = "115104";
    pub const ANDROID_DEVICE: &str = "SM-G977B, beyondx";
    pub const ANDROID_OS_SDK_VERSION: u8 = 31;

    // Linux stable.
    pub const LINUX_OS_VERSION: &str = "5.16.11";

    let identify_properties = IdentifyProperties::default().android(
        ANDROID_CLIENT_VERSION,
        ANDROID_CLIENT_BUILD_NUMBER,
        ANDROID_DEVICE,
        ANDROID_OS_SDK_VERSION,
    );

    let identify_properties = IdentifyProperties::default().linux(LINUX_OS_VERSION);

    let (shard, events) = Shard::builder(token, intents)
        .gateway_url(Some("wss://gateway.discord.gg".to_string()))
        .identify_properties(identify_properties)
        .presence(UpdatePresencePayload::new(
            vec![],
            false,
            None,
            Status::Online,
        )?)
        .build();

    let shard = Arc::new(shard);
    let shard2 = Arc::clone(&shard);
    let mut stream = Box::pin(Gateway::new(shard2, events));
    let mut input_stream = EventStream::new();

    let res = run_app(&mut terminal, shard, stream, input_stream, app).await;

    // restore terminal
    disable_raw_mode()?;

    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;

    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
}

use twilight_gateway::Event as GatewayEvent2;

enum ControlFlow {
    Break,
    Continue,
}

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    mut shard: Arc<Shard>,
    mut gateway: Pin<Box<Gateway>>,
    mut input_stream: EventStream,
    mut app: App,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, &app))?;

        tokio::select! {
            maybe_event = gateway.next() => {
                handle_gateway(terminal, &shard, &mut app, maybe_event).await?;
            }
            maybe_event = input_stream.next() => {
                if matches!(handle_input(terminal, &shard, &mut app, maybe_event).await?, ControlFlow::Break) {
                    shard.shutdown();

                    return Ok(());
                }
            }
        }
    }
}

use std::collections::btree_map::Entry;

async fn handle_gateway<B: Backend>(
    terminal: &mut Terminal<B>,
    shard: &Shard,
    mut app: &mut App,
    mut maybe_event: Option<GatewayEvent>,
) -> Result<ControlFlow> {
    if let Some(event) = maybe_event {
        match event {
            GatewayEvent::Event(GatewayEvent2::ChannelCreate(channel)) => {
                let channel_id = channel.id();
                let channel_name = match channel.name() {
                    Some(name) => name,
                    None => return Ok(ControlFlow::Continue),
                };

                let channel_name = Box::from(channel_name);

                match app.channel_map.entry(channel_id) {
                    Entry::Vacant(entry) => {
                        let channel = Arc::new(RwLock::new(Channel {
                            id: channel_id,
                            messages: BTreeMap::new(),
                            name: channel_name,
                        }));

                        entry.insert(channel);
                    }
                    Entry::Occupied(_entry) => {}
                }
            }
            GatewayEvent::Event(GatewayEvent2::GuildCreate(guild)) => {
                let _guild_id = guild.id;
                let _guild_name = guild.name.clone();

                for channel in guild.channels.iter() {
                    let channel_id = channel.id();
                    let channel_name = channel.name();
                    let channel_name = Box::from(channel_name);

                    match app.channel_map.entry(channel_id) {
                        Entry::Vacant(entry) => {
                            let channel = Arc::new(RwLock::new(Channel {
                                id: channel_id,
                                messages: BTreeMap::new(),
                                name: channel_name,
                            }));

                            entry.insert(channel);
                        }
                        Entry::Occupied(_entry) => {}
                    }
                }
            }
            GatewayEvent::Event(GatewayEvent2::MessageCreate(message)) => {
                let author_id = message.author.id;
                let author_username = message.author.name.clone();

                let channel_id = message.channel_id;

                let message_content = message.content.clone();
                let message_id = message.id;

                let user = Arc::new(User {
                    username: author_username,
                    id: author_id,
                });

                let user2 = Arc::clone(&user);

                let message = Arc::new(RwLock::new(Message {
                    author: user2,
                    content: message_content,
                    id: message_id,
                }));

                let message2 = Arc::clone(&message);

                match app.channel_map.entry(channel_id) {
                    Entry::Vacant(entry) => {
                        if let Some(channel_name) = shard
                            .config()
                            .http_client()
                            .channel(channel_id)
                            .exec()
                            .await?
                            .model()
                            .await?
                            .name()
                        {
                            let channel = Arc::new(RwLock::new(Channel {
                                id: channel_id,
                                messages: BTreeMap::new(),
                                name: Box::from(channel_name),
                            }));

                            channel.write().messages.insert(message_id, message2);

                            entry.insert(channel);
                        }
                    }
                    Entry::Occupied(entry) => {
                        entry.get().write().messages.insert(message_id, message2);
                    }
                }

                app.message_map.insert(message_id, message);
                app.user_map.insert(author_id, user);
            }
            event => {
                app.messages.push(format!("{event:?}"));
            }
        }
    }

    Ok(ControlFlow::Continue)
}

async fn handle_input<B: Backend>(
    terminal: &mut Terminal<B>,
    shard: &Shard,
    mut app: &mut App,
    mut maybe_event: Option<io::Result<Event>>,
) -> Result<ControlFlow> {
    if let Some(Ok(Event::Key(key))) = maybe_event {
        match app.input_mode {
            InputMode::Normal => match key.code {
                KeyCode::Char('e') => {
                    app.input_mode = InputMode::Editing;
                }
                KeyCode::Char('q') => {}
                _ => {}
            },
            InputMode::Editing => match key.code {
                KeyCode::Enter => {
                    let message: String = app.input.drain(..).collect();

                    if message.starts_with('/') {
                        let command = &message[1..];
                        let mut parts = command.split_whitespace();

                        match (parts.next(), parts.next()) {
                            (Some("channel"), Some(id)) => {
                                if let Some(id) = id.parse().ok() {
                                    app.current_channel = ChannelId::new(id);
                                }
                            }
                            _ => {}
                        }
                    } else {
                        if let Some(channel_id) = app.current_channel.as_ref() {
                            shard
                                .config()
                                .http_client()
                                .create_message(*channel_id)
                                .content(&message)?
                                .exec()
                                .await?;
                        }
                    }
                }
                KeyCode::Char(c) => {
                    app.input.push(c);
                }
                KeyCode::Backspace => {
                    app.input.pop();
                }
                KeyCode::Esc => {
                    return Ok(ControlFlow::Break);
                }
                _ => {}
            },
        }
    }

    Ok(ControlFlow::Continue)
}

fn ui<B: Backend>(frame: &mut Frame<B>, app: &App) {
    let channel_list_width = Constraint::Length(10);
    let seperator = Constraint::Length(1);
    let messages_list_width = Constraint::Min(1);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .margin(1)
        .constraints([channel_list_width, seperator, messages_list_width].as_ref())
        .split(frame.size());

    let mut channels: Vec<ListItem> = vec![];

    for (id, channel) in app.channel_map.iter() {
        let current = if let Some(channel_id) = app.current_channel.as_ref() {
            id == channel_id
        } else {
            false
        };

        let name = &channel.read().name;
        let name = if current {
            Span::styled(format!("{}", name), Style::default().fg(Color::Yellow))
        } else {
            Span::raw(format!("{}", name))
        };

        let spans = Spans::from(vec![name]);
        let item = vec![spans];

        channels.push(ListItem::new(item));
    }

    let channels = List::new(channels).start_corner(Corner::BottomLeft);

    let mut messages: Vec<ListItem> = vec![];

    if let Some(channel_id) = app.current_channel.as_ref() {
        if let Some(channel) = app.channel_map.get(channel_id) {
            for (i, (id, message)) in channel.read().messages.iter().rev().enumerate() {
                let message = message.read();
                let author = &message.author.username;
                let author = Span::styled(format!("{}", author), Style::default().fg(Color::Red));
                let timestamp = Span::styled(
                    format!(" 69:69:69 69/69/69"),
                    Style::default().fg(Color::DarkGray),
                );

                let content = &message.content;
                let content = Span::raw(format!("{}", content));
                //let content = Span::styled(format!("{}", content), Style::default().fg(Color::DarkGray));

                let author = Spans::from(vec![author, timestamp]);
                let author = vec![author];

                let content = Spans::from(content);
                let content = vec![content];

                messages.push(ListItem::new("\n"));
                messages.push(ListItem::new(content));
                messages.push(ListItem::new(author));
            }
        }
    }

    let message_list_height = Constraint::Min(1);
    let textarea_height = Constraint::Length(1);

    let chunks2 = Layout::default()
        .direction(Direction::Vertical)
        .constraints([message_list_height, seperator, textarea_height].as_ref())
        .split(chunks[0]);

    let chunks3 = Layout::default()
        .direction(Direction::Vertical)
        .constraints([message_list_height, seperator, textarea_height].as_ref())
        .split(chunks[2]);

    let messages = List::new(messages).start_corner(Corner::BottomLeft);
    let textarea = if app.input.is_empty() {
        Paragraph::new("message").style(Style::default().fg(Color::DarkGray))
    } else {
        Paragraph::new(&*app.input)
    };

    frame.render_widget(channels, chunks2[0]);
    frame.render_widget(messages, chunks3[0]);
    frame.render_widget(textarea, chunks3[2]);

    frame.set_cursor(chunks3[2].x + app.input.width() as u16, chunks3[2].y);
}
