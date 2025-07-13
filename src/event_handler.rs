use anyhow::Result;
use anyhow::bail;
use governor::DefaultDirectRateLimiter;
use governor::Quota;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::mem;
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
use twilight_http::request::application::interaction;
use twilight_model::application::interaction::Interaction;
use twilight_model::application::interaction::InteractionData;
use twilight_model::application::interaction::application_command::CommandData;
use twilight_model::gateway::payload::incoming::GuildCreate;
use twilight_model::gateway::payload::incoming::PresenceUpdate;
use twilight_model::gateway::payload::outgoing::UpdatePresence;
use twilight_model::gateway::payload::outgoing::update_presence::UpdatePresencePayload;
use twilight_model::gateway::presence;
use twilight_model::gateway::presence::Activity;
use twilight_model::gateway::presence::ActivityType;
use twilight_model::gateway::presence::ClientStatus;
use twilight_model::gateway::presence::MinimalActivity;
use twilight_model::gateway::presence::Presence;
use twilight_model::gateway::presence::Status;
use twilight_model::id::{
    Id,
    marker::{GuildMarker, UserMarker},
};

use crate::discord_utils::purge_guild_roles;
use crate::events::easter;
use crate::events::update_roles_by_activity;
use crate::interactions::command::GuildRulesList;
use crate::interactions::command::XkcdCommand;
use crate::rules_handler::{GuildRules, load_rules};

pub static SHUTDOWN: AtomicBool = AtomicBool::new(false);
pub const DEBOUNCE_DELAY: Duration = Duration::from_secs(10);

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
                    self.guild_role_purge(guild_id).await;
                    self.lazy_null(guild_id).await;
                }
                GuildCreate::Unavailable(_) => (),
            },
            Event::InteractionCreate(interaction) => {
                let mut interaction = interaction.0;
                let data = match mem::take(&mut interaction.data) {
                    Some(InteractionData::ApplicationCommand(data)) => *data,
                    _ => {
                        tracing::warn!("ignoring non-command interaction");
                        return Err(anyhow::format_err!("asdasd"));
                    }
                };
                self.handle_command(interaction, data).await;
            }
            _ => (),
        };
        Ok({})
    }

    /// Handle a command interaction.
    pub async fn handle_command(
        &self,
        interaction: Interaction,
        data: CommandData,
    ) -> anyhow::Result<()> {
        match &*data.name {
            "xkcd" => XkcdCommand::handle(interaction, data, &self.http_client).await,
            "list-guild-rules" => {
                GuildRulesList::handle(interaction, data, &self.http_client, &self.rules).await
            }
            name => bail!("unknown command: {}", name),
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

    pub async fn guild_role_purge(&self, guild_id: Id<GuildMarker>) {
        tokio::spawn(purge_guild_roles(
            self.http_client.clone(),
            self.cache.clone(),
            self.presence_queue.clone(),
            guild_id.clone(),
        ));
    }

    pub async fn lazy_null(&self, guild_id: Id<GuildMarker>) {
        tokio::spawn(easter(
            self.http_client.clone(),
            self.rate_limiter.clone(),
            self.cache.clone(),
            guild_id,
        ));
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
        tracing::info!(?item, shard = ?shard.id(), "Received Event");

        match &item {
            Ok(event) => {
                bot.cache.update(event);
            }
            _ => (),
        };

        match item {
            Ok(Event::GatewayClose(_)) if SHUTDOWN.load(Ordering::Relaxed) => break,
            Ok(event) => bot.process_event(event).await?,
            Err(source) => {
                tracing::error!(?source, "error receiving event");
                continue;
            }
        };
    }

    Ok(vec![presence_handler_task])
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
