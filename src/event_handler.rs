use anyhow::Result;
use governor::DefaultDirectRateLimiter;
use governor::{Quota, RateLimiter, clock::DefaultClock, state::InMemoryState};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::num::NonZeroU32;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::Join;
use tokio::{signal, sync::Mutex, task::JoinHandle, time::sleep};
use twilight_cache_inmemory::{InMemoryCache, ResourceType};
use twilight_gateway::{CloseFrame, Config, Event, EventTypeFlags, Intents, Shard, StreamExt as _};
use twilight_http::Client;
use twilight_model::gateway::payload::incoming::PresenceUpdate;
use twilight_model::gateway::presence::Activity;
use twilight_model::id::{
    Id,
    marker::{GuildMarker, RoleMarker, UserMarker},
};

use crate::rules_handler::{GuildRules, Rule, load_rules};

pub static SHUTDOWN: AtomicBool = AtomicBool::new(false);

pub struct DebounceTask {
    pub handle: JoinHandle<()>,
}

pub struct Bot {
    pub rules: BTreeMap<u64, GuildRules>,
    pub cache: Arc<InMemoryCache>,
    pub debounce_tasks: Arc<Mutex<HashMap<(Id<GuildMarker>, Id<UserMarker>), DebounceTask>>>,
    pub presence_queue: Arc<Mutex<HashMap<(Id<GuildMarker>, Id<UserMarker>), PresenceUpdate>>>,
    pub rate_limiter: Arc<DefaultDirectRateLimiter>,
}

impl Bot {
    pub fn new() -> Self {
        let cache = Arc::new(
            InMemoryCache::builder()
                .resource_types(ResourceType::all())
                .build(),
        );
        let debounce_tasks = Arc::new(Mutex::new(HashMap::new()));
        let rate_limiter = Arc::new(DefaultDirectRateLimiter::direct(Quota::per_minute(
            NonZeroU32::new(10u32).unwrap(),
        ))); // 5 role changes per second
        let presence_queue = Arc::new(Mutex::new(HashMap::new()));
        let rules = load_rules();

        Self {
            rules,
            cache,
            debounce_tasks,
            presence_queue,
            rate_limiter,
        }
    }

    /// Function to eat up an event and decide how to handle it, runs as part of the main thread, should not block  
    pub async fn process_event(&self, event: Event) {
        self.cache.update(&event);

        match event {
            Event::PresenceUpdate(presence_update) => {
                // self.handle_presence_update(*presence_update).await;

                let mut queue = self.presence_queue.lock().await;
                let guild_id = presence_update.guild_id;
                let user_id = presence_update.user.id();
                queue.insert((guild_id, user_id), *presence_update);
            }
            _ => (),
        }
    }

    /// runs as a separate thread to handle the presence queue
    /// locks the queue when handling it because it mutates the queue
    pub async fn presence_update_loop_handler(&self) {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            let mut map = self.presence_queue.lock().await;

            for ((guild_id, user_id), presence_update) in map.drain() {
                tracing::debug!(?guild_id, ?user_id, "Processing latest activity for user:");
                self.handle_presence_update(presence_update).await
            }
        }
    }

    /// the actual logic to change roles for users based on presence
    pub async fn handle_presence_update(&self, presence: PresenceUpdate) {
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
}

/// entry point for the shard to run, the "main" function
pub async fn runner(mut shard: Shard, bot: Arc<Bot>) -> Vec<JoinHandle<()>> {
    // Processing task
    let presence_handler_bot = bot.clone();
    let presence_handler_task =
        tokio::spawn(async move { presence_handler_bot.presence_update_loop_handler().await });

    // Event loop

    while let Some(item) = shard.next_event(EventTypeFlags::all()).await {
        let event = match item {
            Ok(Event::GatewayClose(_)) if SHUTDOWN.load(Ordering::Relaxed) => break,
            Ok(event) => bot.process_event(event).await,
            Err(source) => {
                tracing::error!(?source, "error receiving event");
                continue;
            }
        };

        tracing::debug!(?event, shard = ?shard.id(), "received event");
    }

    vec![presence_handler_task]
}

// Configuration
const ROLE_ID: Id<RoleMarker> = Id::new(123456789012345678); // Your target role ID
const GAME_NAME: &str = "My Cool Game";
const DEBOUNCE_DELAY: Duration = Duration::from_secs(60);

// Represents a scheduled debounce task

mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_hashmap() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();

        let mut h = HashMap::new();
        h.insert(0, 69);
        assert_eq!(h[&0], 69);
        // tracing::debug!(?h);

        h.insert(0, 42);
        assert_eq!(h[&0], 42);
        // tracing::debug!(?h);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_arc_cloning() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();
        let h: Arc<Mutex<HashMap<u64, u64>>> = Arc::new(Mutex::new(HashMap::new()));
        let h2 = h.clone();
        let mut m = h2.lock().await;
        m.insert(0, 42);
        std::mem::drop(m);

        let m2 = h.lock().await;
        assert_eq!(m2[&0], 42);

        tracing::debug!(?h);
        tracing::debug!(?m2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn time_play() {
        let ts = SystemTime::now();

        sleep(Duration::new(2, 0)).await;

        match ts.elapsed() {
            Ok(t) => assert!(t.as_secs() > 1),
            Err(_) => panic!("WUT"),
        }
    }
}
