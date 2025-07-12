mod config_handler;
use anyhow::Result;
use config_handler::get_config;
use governor::DefaultDirectRateLimiter;
use governor::{Quota, RateLimiter, clock::DefaultClock, state::InMemoryState};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use tokio::{signal, sync::Mutex, task::JoinHandle, time::sleep};
use twilight_cache_inmemory::{InMemoryCache, ResourceType};
use twilight_gateway::{CloseFrame, Config, Event, EventTypeFlags, Intents, Shard, StreamExt as _};
use twilight_http::Client;
use twilight_model::gateway::payload::incoming::PresenceUpdate;
use twilight_model::id::{
    Id,
    marker::{GuildMarker, RoleMarker, UserMarker},
};

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

#[tokio::main]
async fn main() -> Result<()> {
    let config = get_config()?;
    let token = config.discord_token;
    // Initialize the tracing subscriber.
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let intents = Intents::GUILD_PRESENCES | Intents::GUILDS;
    let client = Client::new(token.clone());
    let config = Config::new(token, intents);

    let shards =
        twilight_gateway::create_recommended(&client, config, |_, builder| builder.build()).await?;
    let mut senders = Vec::with_capacity(shards.len());
    let mut tasks = Vec::with_capacity(shards.len());

    tracing::debug!("Spawned Shards: {}", &shards.len());

    for shard in shards {
        senders.push(shard.sender());
        tasks.push(tokio::spawn(runner(shard)));
    }

    signal::ctrl_c().await?;
    SHUTDOWN.store(true, Ordering::Relaxed);
    for sender in senders {
        // Ignore error if shard's already shutdown.
        _ = sender.close(CloseFrame::NORMAL);
    }

    for jh in tasks {
        _ = jh.await;
    }

    Ok({})
}

async fn runner(mut shard: Shard) {
    let bot = Bot::new();
    let presences: Arc<
        Mutex<HashMap<u64, Box<twilight_model::gateway::payload::incoming::PresenceUpdate>>>,
    > = Arc::new(Mutex::new(HashMap::new()));

    // Processing task
    tokio::spawn({
        let presences = Arc::clone(&presences);
        async move {
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let mut map = presences.lock().await;
                for (user_id, presence) in map.drain() {
                    println!("Processing latest activity for user {}:", user_id);
                    for activity in &presence.activities {
                        println!(" - {} ({:?})", activity.name, activity.kind);
                    }
                }
            }
        }
    });

    // Event loop

    while let Some(item) = shard.next_event(EventTypeFlags::all()).await {
        let event = match item {
            Ok(Event::GatewayClose(_)) if SHUTDOWN.load(Ordering::Relaxed) => break,
            Ok(Event::PresenceUpdate(presence_update)) => {
                tracing::debug!(?presence_update);
                let user_id = match &presence_update.user {
                    twilight_model::gateway::presence::UserOrId::User(user) => user.id.get(),
                    twilight_model::gateway::presence::UserOrId::UserId { id } => id.get(),
                };
                let mut map = presences.lock().await;
                map.insert(user_id, presence_update);
                continue;
            }
            Ok(event) => event,
            Err(source) => {
                tracing::warn!(?source, "error receiving event");

                continue;
            }
        };
        // You'd normally want to spawn a new tokio task for each event and
        // handle the event there to not block the shard.
        tracing::debug!(?event, shard = ?shard.id(), "received event");

        bot.process_event(event).await;
    }
}

// Configuration
const ROLE_ID: Id<RoleMarker> = Id::new(123456789012345678); // Your target role ID
const GAME_NAME: &str = "My Cool Game";
const DEBOUNCE_DELAY: Duration = Duration::from_secs(60);

// Represents a scheduled debounce task
struct DebounceTask {
    pub handle: JoinHandle<()>,
}

struct Bot {
    pub cache: Arc<InMemoryCache>,
    pub debounce_tasks: Arc<Mutex<HashMap<(Id<GuildMarker>, Id<UserMarker>), DebounceTask>>>,
    pub rate_limiter: Arc<DefaultDirectRateLimiter>,
}

impl Bot {
    fn new() -> Self {
        let cache = Arc::new(
            InMemoryCache::builder()
                .resource_types(ResourceType::all())
                .build(),
        );
        let debounce_tasks = Arc::new(Mutex::new(HashMap::new()));
        let rate_limiter = Arc::new(DefaultDirectRateLimiter::direct(Quota::per_second(
            NonZeroU32::new(2u32).unwrap(),
        ))); // 5 role changes per second

        Self {
            cache,
            debounce_tasks,
            rate_limiter,
        }
    }

    async fn handle_presence_update(&self, presence: PresenceUpdate) {
        let guild_id = presence.guild_id;
        let user_id = presence.user.id();

        let key = (guild_id, user_id);
        let mut tasks = self.debounce_tasks.lock().await;

        // Cancel existing task if exists
        if let Some(task) = tasks.remove(&key) {
            task.handle.abort();
        }

        // Schedule new debounce task
        let cache = self.cache.clone();
        let limiter = self.rate_limiter.clone();
        let task_handle = tokio::spawn(async move {
            sleep(DEBOUNCE_DELAY).await;

            let activities = &presence.activities;
            let in_game = activities.iter().any(|a| a.name == GAME_NAME);

            if let Some(member) = cache.member(guild_id, user_id) {
                // let has_role = member.roles.contains(&ROLE_ID);
                let has_role = member.roles().contains(&ROLE_ID);

                if in_game && !has_role {
                    limiter.until_ready().await;
                    println!("Would add role {:?} to user {:?}", ROLE_ID, user_id);
                    // Actual API call here
                } else if !in_game && has_role {
                    limiter.until_ready().await;
                    println!("Would remove role {:?} from user {:?}", ROLE_ID, user_id);
                    // Actual API call here
                } else {
                    println!("No role change needed for user {:?}", user_id);
                }
            } else {
                println!("Member not found in cache for user {:?}", user_id);
            }
        });

        tasks.insert(
            key,
            DebounceTask {
                handle: task_handle,
            },
        );
    }

    async fn process_event(&self, event: Event) {
        self.cache.update(&event);

        match event {
            Event::PresenceUpdate(presence_update) => {
                self.handle_presence_update(*presence_update).await
            }
            _ => (),
        }
    }
}

#[test]
fn test_hashmap() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let mut h = HashMap::new();
    h.insert(0, 69);
    tracing::debug!(?h);

    h.insert(0, 42);

    tracing::debug!(?h);
}
