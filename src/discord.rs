use fst::{
    automaton::{Automaton, Levenshtein, StartsWith, Str, Union},
    IntoStreamer, Map, Streamer,
};

use std::{borrow::Cow, collections::BTreeMap};
use strife_discord::{
    cache::InMemoryCache,
    gateway::{error::ReceiveMessageError, Config, Event, Intents, Shard as Gateway, ShardId},
    http::Client as Rest,
    model::{
        channel::ChannelType,
        gateway::{payload::outgoing::update_presence::UpdatePresencePayload, presence::Status},
        id::{marker::ChannelMarker, Id},
    },
};

bitflags::bitflags! {
    /// Cache damage tracking.
    struct CacheDamage: u32 {
        const MESSAGE = 1 << 1;
        const CHANNEL = 1 << 2;
        const CHANNEL_SEARCH = 1 << 3;
        const GUILD = 1 << 4;
        const USER = 1 << 5;
    }
}

/// A discord client.
pub struct Client {
    pub cache: InMemoryCache,
    pub current_channel: Option<Id<ChannelMarker>>,
    pub rest: Rest,
    pub gateway: Gateway,

    /// Fuzzy finder channel map.
    channel_map: ChannelMap,

    /// Cache damage tracking.
    damage: CacheDamage,
}

impl Client {
    /// Create a new client.
    pub fn new(token: impl Into<String>) -> Self {
        let token = token.into();
        let rest = Rest::new(token.clone());
        let config = Config::builder(token, Intents::empty())
            .presence(UpdatePresencePayload {
                activities: Vec::new(),
                afk: false,
                since: None,
                status: Status::DoNotDisturb,
            })
            .build();

        let gateway = Gateway::with_config(ShardId::ONE, config);

        Self {
            cache: InMemoryCache::default(),
            current_channel: None,
            rest,
            gateway,
            channel_map: ChannelMap::default(),
            damage: CacheDamage::empty(),
        }
    }

    /// Process the next event.
    pub async fn next_event(&mut self) -> Result<Event, ReceiveMessageError> {
        let event = self.gateway.next_event().await.map_err(|error| {
            if std::env::var("STRIFE_DEBUG").is_ok() {
                println!("{error:?}");
            }

            error
        })?;

        self.cache.update(&event);

        match &event {
            Event::ChannelCreate(channel) => self.update_channel_damage(channel.id),
            Event::ChannelDelete(channel) => self.update_channel_damage(channel.id),
            Event::ChannelUpdate(channel) => self.update_channel_damage(channel.id),

            Event::MessageCreate(message) => self.update_message_damage(message.channel_id),
            Event::MessageDelete(message) => self.update_message_damage(message.channel_id),
            Event::MessageDeleteBulk(message) => self.update_message_damage(message.channel_id),
            Event::MessageUpdate(message) => self.update_message_damage(message.channel_id),
            _ => {}
        }

        Ok(event)
    }

    /// That's a lotta damage!
    fn take_damage(&mut self, damage: CacheDamage) -> bool {
        let value = self.damage.contains(damage);

        self.damage.remove(damage);

        value
    }

    /// Whether the channel cache has been update.
    pub fn take_channel_cache_damage(&mut self) -> bool {
        self.take_damage(CacheDamage::CHANNEL)
    }

    /// Whether message cache has been updated.
    pub fn take_message_cache_damage(&mut self) -> bool {
        self.take_damage(CacheDamage::MESSAGE)
    }

    /// Whether channel cache damage.
    fn update_channel_damage(&mut self, _channel_id: Id<ChannelMarker>) {
        self.damage
            .insert(CacheDamage::CHANNEL | CacheDamage::CHANNEL_SEARCH);
    }

    /// Update message cache damage.
    fn update_message_damage(&mut self, channel_id: Id<ChannelMarker>) {
        let Some(current_channel_id) = self.current_channel else {
            return;
        };

        if current_channel_id == channel_id {
            self.damage.insert(CacheDamage::MESSAGE);
        }
    }

    /// Fuzzy search for a channel
    //
    // TODO: Cache the FST map & damage tracking.
    pub fn search_channel(&mut self, query: &str) -> Vec<(String, Id<ChannelMarker>)> {
        if self.take_damage(CacheDamage::CHANNEL_SEARCH) {
            self.channel_map.update(&self.cache);
        }

        self.channel_map.search(query)
    }
}

#[derive(Default)]
pub struct ChannelMap {
    map: BTreeMap<String, Id<ChannelMarker>>,
    fst: Map<Vec<u8>>,
}

impl ChannelMap {
    pub fn update(&mut self, cache: &InMemoryCache) {
        self.map = cache
            .iter()
            .channels()
            .flat_map(|channel| {
                if !is_text_channel(channel.kind) {
                    return None;
                }

                let name = channel.name.clone()?;
                let id = channel.id;

                Some((name, id))
            })
            .collect();

        let iter = self
            .map
            .iter()
            .map(|(name, id)| (name.as_str(), id.get()))
            .collect::<Vec<_>>();

        self.fst = Map::from_iter(iter).unwrap();
    }

    /// Fuzzy search for a channel.
    pub fn search(&self, query: &str) -> Vec<(String, Id<ChannelMarker>)> {
        let query = sanitize_text_channel_name(query);
        let query = make_query(&query);

        let mut stream = self.fst.search(query).into_stream();
        let mut results = Vec::new();

        while let Some((name, id)) = stream.next() {
            // SAFETY: `name` originated from a valid UTF-8 string.
            let name = unsafe { String::from_utf8_unchecked(name.to_vec()) };

            // SAFETY: `id` originated from a valid channel ID.
            let id = unsafe { Id::new_unchecked(id) };

            results.push((name, id));
        }

        if results.is_empty() {
            results.extend(self.map.clone());
        }

        results
    }
}

/// Build the query for a fuzzy search.
fn make_query(query: &str) -> Union<StartsWith<Str>, Levenshtein> {
    Str::new(query)
        .starts_with()
        .union(Levenshtein::new(query, 1).unwrap())
}

/// Sanitize a channel name for text.
fn sanitize_text_channel_name(name: &str) -> Cow<'_, str> {
    let name = name.trim().trim_matches('-');

    if name.contains([' ', '-']) {
        Cow::Owned(name.replace(' ', "-"))
    } else {
        Cow::Borrowed(name)
    }
}

/// Determine whether a channel type is for text.
fn is_text_channel(kind: ChannelType) -> bool {
    matches!(
        kind,
        ChannelType::GuildText
            | ChannelType::Private
            | ChannelType::Group
            | ChannelType::PublicThread
            | ChannelType::PrivateThread
    )
}
