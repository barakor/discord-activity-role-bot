use anyhow::Result;
use governor::DefaultDirectRateLimiter;
use governor::Quota;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::num::NonZeroU32;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use tokio::{sync::Mutex, task::JoinHandle};
use twilight_cache_inmemory::{InMemoryCache, ResourceType};
use twilight_gateway::{Event, EventTypeFlags, Shard, StreamExt as _};
use twilight_http::Client;
use twilight_model::gateway::payload::incoming::GuildCreate;
use twilight_model::gateway::payload::incoming::PresenceUpdate;
use twilight_model::gateway::payload::outgoing::UpdatePresence;
use twilight_model::gateway::presence;
use twilight_model::gateway::presence::Activity;
use twilight_model::gateway::presence::ActivityType;
use twilight_model::gateway::presence::ClientStatus;
use twilight_model::gateway::presence::Presence;
use twilight_model::gateway::presence::Status;
use twilight_model::id::{
    Id,
    marker::{GuildMarker, UserMarker},
};

use crate::events::update_roles_by_activity;
use crate::rules_handler::{GuildRules, load_rules};

pub static SHUTDOWN: AtomicBool = AtomicBool::new(false);
pub const DEBOUNCE_DELAY: Duration = Duration::from_secs(1);

pub struct DebounceTask {
    pub handle: JoinHandle<()>,
}

pub struct Bot {
    pub http_client: Arc<Client>,
    pub rules: BTreeMap<u64, GuildRules>,
    pub cache: Arc<InMemoryCache>,
    pub debounce_tasks: Arc<Mutex<HashMap<(Id<GuildMarker>, Id<UserMarker>), DebounceTask>>>,
    pub presence_queue: Arc<Mutex<HashMap<(Id<GuildMarker>, Id<UserMarker>), PresenceUpdate>>>,
    pub rate_limiter: Arc<DefaultDirectRateLimiter>,
}

impl Bot {
    pub fn new(http_client: Arc<Client>) -> Self {
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
            http_client,
            rules,
            cache,
            debounce_tasks,
            presence_queue,
            rate_limiter,
        }
    }

    /// Function to eat up an event and decide how to handle it, runs as part of the main thread, should not block  
    pub async fn process_event(&self, event: Event) -> Result<()> {
        match event {
            Event::PresenceUpdate(presence_update) => {
                // self.handle_presence_update(*presence_update).await;

                let mut queue = self.presence_queue.lock().await;
                let guild_id = presence_update.guild_id;
                let user_id = presence_update.user.id();
                queue.insert((guild_id, user_id), *presence_update);
            }
            Event::GuildCreate(guild_create) => match *guild_create {
                GuildCreate::Available(guild_data) => {
                    let guild_id = guild_data.id;
                    self.guild_role_purge(guild_id).await?;
                }
                GuildCreate::Unavailable(_) => (),
            },
            _ => (),
        };
        Ok({})
    }

    /// runs as a separate thread to handle the presence queue
    /// locks the queue when handling it because it mutates the queue
    pub async fn presence_update_loop_handler(&self) {
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let mut map = self.presence_queue.lock().await;

            for ((guild_id, user_id), presence_update) in map.drain() {
                tracing::debug!(
                    ?guild_id,
                    ?user_id,
                    ?presence_update,
                    "Processing latest activity for user:"
                );
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
        let roles_rules = self.rules.clone();
        let http_client = self.http_client.clone();

        let user_activities: BTreeSet<String> = presence
            .activities
            .iter()
            .filter(|activity| activity.kind == ActivityType::Playing)
            .map(|activity| activity.name.to_string())
            .collect();

        let task_handle = tokio::spawn(update_roles_by_activity(
            http_client,
            limiter,
            cache,
            roles_rules,
            guild_id,
            user_id,
            user_activities,
        ));

        tasks.insert(
            key,
            DebounceTask {
                handle: task_handle,
            },
        );
    }

    pub async fn get_all_guild_members(
        &self,
        guild_id: Id<GuildMarker>,
    ) -> Result<Vec<Id<UserMarker>>> {
        let mut after: Option<Id<UserMarker>> = None;
        let mut user_ids = Vec::new();

        loop {
            let members_result = self
                .http_client
                .guild_members(guild_id)
                .limit(1000)
                .after(after.unwrap_or(Id::new(1)))
                .await?;

            let members = members_result.model().await?;

            if members.is_empty() {
                break;
            }

            user_ids.extend(members.iter().map(|m| m.user.id));

            after = Some(members.last().unwrap().user.id);
        }

        Ok(user_ids)
    }

    pub async fn guild_role_purge(&self, guild_id: Id<GuildMarker>) -> Result<()> {
        let guild_members = self.get_all_guild_members(guild_id).await?;
        let mut queue = self.presence_queue.lock().await;
        for user_id in guild_members {
            let (status, activities) = match self.cache.presence(guild_id, user_id) {
                Some(presence) => (
                    presence.status(),
                    presence.activities().iter().map(|x| x.clone()).collect(),
                ),
                None => (Status::Offline, vec![]),
            };
            let presence = Presence {
                activities,
                client_status: ClientStatus {
                    desktop: None,
                    mobile: None,
                    web: None,
                },
                guild_id,
                status,
                user: presence::UserOrId::UserId { id: user_id },
            };
            queue.insert((guild_id, user_id), PresenceUpdate(presence));
        }
        Ok({})
    }
}

/// entry point for the shard to run, the "main" function
pub async fn runner(mut shard: Shard, bot: Arc<Bot>) -> Result<Vec<JoinHandle<()>>> {
    // Processing task
    let presence_handler_bot = bot.clone();
    let presence_handler_task =
        tokio::spawn(async move { presence_handler_bot.presence_update_loop_handler().await });

    // Event loop

    while let Some(item) = shard.next_event(EventTypeFlags::all()).await {
        match &item {
            Ok(event) => {
                bot.cache.update(event);
            }
            _ => (),
        };
        tracing::info!(?item, shard = ?shard.id(), "Received Event");

        match item {
            Ok(Event::GatewayClose(_)) if SHUTDOWN.load(Ordering::Relaxed) => break,
            Ok(Event::Ready(_)) => {
                set_shard_activity(&shard, "Rolling Dice".to_string());
            }
            Ok(event) => bot.process_event(event).await?,
            Err(source) => {
                tracing::error!(?source, "error receiving event");
                continue;
            }
        };
    }

    Ok(vec![presence_handler_task])
}

pub fn set_shard_activity(shard: &Shard, activity: String) {
    let activity = Activity {
        name: activity,
        kind: ActivityType::Playing,
        url: None,
        created_at: None,
        timestamps: None,
        application_id: None,
        details: None,
        state: None,
        emoji: None,
        party: None,
        assets: None,
        secrets: None,
        instance: None,
        flags: None,
        buttons: vec![],
        id: None,
    };

    let presence = UpdatePresence::new(vec![activity], false, None, Status::Online).unwrap();

    shard.command(&presence);
}

#[allow(unused_imports)]
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;
    use tokio::time::sleep;

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
