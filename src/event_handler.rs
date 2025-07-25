use crate::{
    config_handler::GithubConfig,
    discord_utils::{interaction_ack, interaction_end, interaction_response, purge_guild_roles},
    events::{easter, handle_presence_update, user_activities_from_presence},
    interactions::command::{ManageCommand, StorageCommand},
    rules_handler::{GuildRules, load_db, update_roles_names},
};
use anyhow::{Result, bail};
use std::{
    collections::{BTreeMap, HashMap},
    mem,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{Mutex, RwLock},
    task::JoinHandle,
};
use twilight_cache_inmemory::{InMemoryCache, ResourceType};
use twilight_gateway::{Event, EventTypeFlags, Shard, StreamExt as _};
use twilight_http::Client;
use twilight_model::{
    application::interaction::{Interaction, InteractionData, application_command::CommandData},
    gateway::payload::incoming::GuildCreate,
    id::{
        Id,
        marker::{GuildMarker, UserMarker},
    },
};
use twilight_util::builder::InteractionResponseDataBuilder;

pub static SHUTDOWN: AtomicBool = AtomicBool::new(false);
pub const DEBOUNCE_DELAY: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct Bot {
    pub http_client: Arc<Client>,
    pub rules: Arc<RwLock<BTreeMap<u64, GuildRules>>>,
    pub cache: Arc<InMemoryCache>,
    pub presence_update_tasks:
        Arc<Mutex<HashMap<(Id<GuildMarker>, Id<UserMarker>), JoinHandle<()>>>>,
    pub github_config: Option<GithubConfig>,
}

impl Bot {
    pub async fn new(http_client: Arc<Client>, github_config: Option<GithubConfig>) -> Self {
        let cache = Arc::new(
            InMemoryCache::builder()
                .resource_types(ResourceType::all())
                .build(),
        );
        let presence_update_tasks = Arc::new(Mutex::new(HashMap::new()));
        let rules = Arc::new(RwLock::new(load_db(github_config.as_ref()).await));

        Self {
            http_client,
            rules,
            cache,
            presence_update_tasks,
            github_config: github_config,
        }
    }

    /// Function to eat up an event and decide how to handle it
    pub async fn process_event(&self, event: Event) -> Result<()> {
        match event {
            Event::PresenceUpdate(presence_update) => {
                let guild_id = presence_update.guild_id;
                let user_id = presence_update.user.id();
                let user_activities =
                    user_activities_from_presence(presence_update.activities.iter());

                let future = handle_presence_update(
                    self.http_client.clone(),
                    self.rules.clone(),
                    self.cache.clone(),
                    self.presence_update_tasks.clone(),
                    guild_id,
                    user_id,
                    user_activities,
                );
                tokio::spawn(future);
            }
            Event::GuildCreate(guild_create) => match *guild_create {
                GuildCreate::Available(guild_data) => {
                    let guild_id = guild_data.id;
                    tokio::spawn(purge_guild_roles(
                        self.http_client.clone(),
                        self.rules.clone(),
                        self.cache.clone(),
                        self.presence_update_tasks.clone(),
                        guild_id.clone(),
                    ));
                    tokio::spawn(easter(self.http_client.clone(), guild_id.clone()));
                    tokio::spawn(update_roles_names(
                        self.rules.clone(),
                        guild_data.roles,
                        guild_id.into(),
                    ));
                }
                GuildCreate::Unavailable(_) => (),
            },
            Event::InteractionCreate(interaction) => {
                let mut interaction = interaction.0;
                let data = match mem::take(&mut interaction.data) {
                    Some(InteractionData::ApplicationCommand(data)) => *data,
                    _ => {
                        tracing::warn!("ignoring non-command interaction");
                        return Err(anyhow::format_err!("ignoring non-command interaction"));
                    }
                };
                let _ = self.handle_command(interaction, data).await;
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
        interaction_ack(&self.http_client, &interaction).await?;
        let response = match &*data.name {
            "manage" => ManageCommand::handle(&interaction, data, &self.cache, &self.rules).await,
            "storage" => {
                StorageCommand::handle(data, &self.rules, self.github_config.as_ref()).await
            }
            name => bail!("unknown command: {}", name),
        };

        match response {
            Ok(Some(response)) => {
                interaction_response(&self.http_client, &interaction, response).await
            }
            Ok(None) => interaction_end(&self.http_client, &interaction).await,
            Err(e) => {
                tracing::error!(?e, "error handling command");
                let error_response = InteractionResponseDataBuilder::new()
                    .content(format!("Error: {}", e))
                    .build();
                interaction_response(&self.http_client, &interaction, error_response).await
            }
        }
    }
}

/// entry point for the shard to run, the "main" function
pub async fn runner(mut shard: Shard, bot: Arc<Bot>) {
    // Event loop
    while let Some(item) = shard.next_event(EventTypeFlags::all()).await {
        tracing::info!(?item, shard = ?shard.id(), "Received Event");

        match &item {
            Ok(event) => {
                let event = event.clone();
                let bot = bot.clone();
                tokio::spawn(async move { bot.cache.update(&event) });
            }
            _ => (),
        };

        match item {
            Ok(Event::GatewayClose(_)) if SHUTDOWN.load(Ordering::Relaxed) => break,
            Ok(event) => {
                let bot = bot.clone();
                tokio::spawn(async move { bot.process_event(event).await })
            }
            Err(source) => {
                tracing::error!(?source, "error receiving event");
                continue;
            }
        };
    }
}
