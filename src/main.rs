use chrono::{Local, NaiveDateTime, TimeZone};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use discord_gateway_stream::{Gateway, GatewayEvent};
use futures_util::stream::StreamExt;
use std::env;
use std::io;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::Arc;
use tui::backend::{Backend, CrosstermBackend};
use tui::layout::{Constraint, Corner, Direction, Layout};
use tui::style::{Color, Style};
use tui::text::{Span, Spans};
use tui::widgets::{List, ListItem, Paragraph};
use tui::{Frame, Terminal};
use twilight_cache_inmemory::InMemoryCache;
use twilight_gateway::Shard;
use twilight_model::gateway::payload::outgoing::identify::IdentifyProperties;
use twilight_model::gateway::payload::outgoing::update_presence::UpdatePresencePayload;
use twilight_model::gateway::presence::Status;
use twilight_model::gateway::Intents;
use twilight_model::id::{ChannelId, MessageId, UserId};
use unicode_width::UnicodeWidthStr;

enum InputMode {
    Normal,
    Editing,
}

struct App {
    cache: InMemoryCache,
    current_channel: Option<ChannelId>,
    input: String,
    input_mode: InputMode,
}

impl Default for App {
    fn default() -> App {
        let cache = InMemoryCache::builder().message_cache_size(100).build();
        let current_channel = None;
        let input = String::new();
        let input_mode = InputMode::Editing;

        App {
            cache,
            current_channel,
            input,
            input_mode,
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

enum ControlFlow {
    Break,
    Continue,
}

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    shard: Arc<Shard>,
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

async fn handle_gateway<B: Backend>(
    terminal: &mut Terminal<B>,
    shard: &Shard,
    mut app: &mut App,
    maybe_event: Option<GatewayEvent>,
) -> Result<ControlFlow> {
    if let Some(GatewayEvent::Event(event)) = maybe_event {
        app.cache.update(&event);
    }

    Ok(ControlFlow::Continue)
}

async fn handle_input<B: Backend>(
    terminal: &mut Terminal<B>,
    shard: &Shard,
    mut app: &mut App,
    maybe_event: Option<io::Result<Event>>,
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
                    let trimmed = app.input.trim();

                    if trimmed.is_empty() {
                        return Ok(ControlFlow::Continue);
                    }

                    let message = trimmed.to_string();

                    app.input.clear();

                    if message.starts_with('/') {
                        let command = &message[1..];
                        let mut parts = command.split_whitespace();

                        match (parts.next(), parts.next()) {
                            (Some("channel"), Some(id)) => {
                                if let Some(id) = id.parse::<u64>().ok() {
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
    let channels = List::new(channels).start_corner(Corner::BottomLeft);

    let mut messages: Vec<ListItem> = vec![];
    let mut iter = app
        .current_channel
        .as_ref()
        .and_then(|channel_id| app.cache.channel_messages(*channel_id))
        .into_iter()
        .flatten();

    for message_id in iter {
        if let Some(message) = app.cache.message(message_id) {
            let author_id = message.author();

            let nickname = message.member().and_then(|member| member.nick.clone());

            let username = app.cache.user(author_id).map(|user| user.name.clone());
            let username = username.as_deref();
            let username = username.unwrap_or("Unresolved").to_string();
            let username = Span::styled(username, Style::default().fg(Color::Red));

            let dt = NaiveDateTime::from_timestamp(message.timestamp().as_secs(), 0);
            let dt = Local.from_utc_datetime(&dt);
            let timestamp = Span::styled(
                dt.format("%H:%M:%S %d/%m/%Y").to_string(),
                Style::default().fg(Color::DarkGray),
            );

            let header = if let Some(nickname) = nickname {
                let nickname = Span::styled(nickname, Style::default().fg(Color::Red));

                Spans::from(vec![
                    nickname,
                    Span::raw(" ("),
                    username,
                    Span::raw(") "),
                    timestamp,
                ])
            } else {
                Spans::from(vec![username, Span::raw(" "), timestamp])
            };

            let content = message.content();
            let content = Span::raw(format!("{}", content));
            let content = if message.edited_timestamp().is_some() {
                let edited = Span::styled("(edited)", Style::default().fg(Color::DarkGray));

                Spans::from(vec![content, Span::raw(" "), edited])
            } else {
                Spans::from(content)
            };

            messages.push(ListItem::new("\n"));
            messages.push(ListItem::new(content));
            messages.push(ListItem::new(header));
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
