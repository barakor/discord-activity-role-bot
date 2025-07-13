use governor::DefaultDirectRateLimiter;
use governor::Quota;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::num::NonZeroU32;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use twilight_model::gateway::payload::outgoing::UpdatePresence;
use twilight_model::gateway::presence::Activity;
use twilight_model::gateway::presence::Status;

use tokio::{sync::Mutex, task::JoinHandle, time::sleep};
use twilight_cache_inmemory::{InMemoryCache, ResourceType};
use twilight_gateway::{Event, EventTypeFlags, Shard, StreamExt as _};
use twilight_http::Client;
use twilight_model::gateway::payload::incoming::PresenceUpdate;
use twilight_model::gateway::presence::ActivityType;
use twilight_model::id::{
    Id,
    marker::{GuildMarker, RoleMarker, UserMarker},
};

use crate::rules_handler::{GuildRules, load_rules};

pub static SHUTDOWN: AtomicBool = AtomicBool::new(false);
const DEBOUNCE_DELAY: Duration = Duration::from_secs(1);

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
    pub async fn process_event(&self, event: Event) {
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
        let task_handle = tokio::spawn(async move {
            sleep(DEBOUNCE_DELAY).await;

            let activities: BTreeSet<String> = presence
                .activities
                .iter()
                .filter(|activity| activity.kind == ActivityType::Playing)
                .map(|activity| activity.name.to_string())
                .collect();

            let guild_rules = match roles_rules.get(&guild_id.get()) {
                Some(guild_rules) => guild_rules,
                None => return,
            };
            let managed_roles: BTreeSet<u64> =
                guild_rules.all_rules().iter().map(|r| r.role_id).collect();

            let rules_to_assign = guild_rules.matching_rules(activities);

            let roles_ids_to_assign: BTreeSet<u64> =
                rules_to_assign.iter().map(|rule| rule.role_id).collect();

            if let Some(member) = cache.member(guild_id, user_id) {
                let user_roles: BTreeSet<u64> = member
                    .roles()
                    .iter()
                    .map(|role_id| role_id.get())
                    .filter(|r| managed_roles.contains(r))
                    .collect();

                let roles_to_add = roles_ids_to_assign.difference(&user_roles).cloned();
                let roles_to_remove = user_roles.difference(&roles_ids_to_assign).cloned();

                for rid in roles_to_add {
                    let role_id: Id<RoleMarker> = Id::new(rid);

                    tracing::warn!("Assigning Role {role_id:?} to {user_id:?} in {guild_id:?}");
                    limiter.until_ready().await;
                    let r = http_client
                        .add_guild_member_role(guild_id, user_id, role_id)
                        .await;

                    match r {
                        Err(e) => tracing::error!(?e, "Couldn't add role"),
                        Ok(_) => (),
                    };
                }

                for rid in roles_to_remove {
                    let role_id: Id<RoleMarker> = Id::new(rid);

                    tracing::warn!("Removing Role {role_id:?} to {user_id:?} in {guild_id:?}");
                    limiter.until_ready().await;
                    let r = http_client
                        .remove_guild_member_role(guild_id, user_id, role_id)
                        .await;

                    match r {
                        Err(e) => tracing::error!(?e, "Couldn't remove role"),
                        Ok(_) => (),
                    };
                }
            } else {
                tracing::error!("Member not found in cache for user {user_id:?}");
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
        match &item {
            Ok(event) => {
                bot.cache.update(event);
            }
            _ => (),
        };

        let event = match item {
            Ok(Event::GatewayClose(_)) if SHUTDOWN.load(Ordering::Relaxed) => break,
            Ok(Event::Ready(_)) => {
                set_shard_activity(&shard, "Rolling Dice".to_string());
            }
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

mod tests {

    #[allow(unused_imports)]
    use super::*;
    #[allow(unused_imports)]
    use std::time::SystemTime;

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
