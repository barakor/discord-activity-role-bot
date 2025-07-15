mod config_handler;
mod discord_utils;
mod event_handler;
mod events;
mod interactions;
mod rules_handler;

use anyhow::Result;
use config_handler::get_config;
use event_handler::runner;
use std::sync::{Arc, atomic::Ordering};
use tokio::signal;
use twilight_gateway::{CloseFrame, ConfigBuilder, Intents, Shard};
use twilight_http::Client;
use twilight_interactions::command::CreateCommand;
use twilight_model::{
    gateway::{
        payload::outgoing::update_presence::UpdatePresencePayload,
        presence::{ActivityType, MinimalActivity, Status},
    },
    id::{Id, marker::GuildMarker},
};

use crate::{
    config_handler::EnvConfig,
    event_handler::{Bot, SHUTDOWN},
    interactions::command::{GuildRulesList, XkcdCommand},
};

async fn boot_shards(config: EnvConfig) -> Result<(Client, Vec<Shard>)> {
    let token = config.discord_token;
    // Initialize the tracing subscriber.

    let intents = Intents::GUILD_PRESENCES | Intents::GUILDS | Intents::GUILD_MEMBERS;
    let client = Client::new(token.clone());
    let config = ConfigBuilder::new(token, intents)
        .presence(bot_presence("Rolling Roles".into()))
        .build();

    let shards: Vec<Shard> =
        twilight_gateway::create_recommended(&client, config, |_, builder| builder.build())
            .await?
            .collect();

    tracing::debug!("Spawned Shards: {}", &shards.len());

    Ok((client, shards))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let config = get_config()?;

    let (client, shards) = boot_shards(config).await?;

    // Register global commands.
    let guild_id: Id<GuildMarker> = Id::new(1104894380080365710);
    let commands = [
        XkcdCommand::create_command().into(),
        GuildRulesList::create_command().into(),
    ];
    let application = client
        .current_user_application()
        .await
        .unwrap()
        .model()
        .await
        .unwrap();
    let interaction_client = client.interaction(application.id);

    tracing::info!("logged as {} with ID {}", application.name, application.id);

    if let Err(error) = interaction_client
        .set_guild_commands(guild_id, &commands)
        .await
    {
        tracing::error!(?error, "failed to register commands");
    }

    let mut senders = Vec::with_capacity(shards.len());
    let mut tasks = Vec::with_capacity(shards.len());

    tracing::debug!("Spawned Shards: {}", &shards.len());
    let bot = Arc::new(Bot::new(Arc::new(client)));

    for shard in shards {
        senders.push(shard.sender());
        tasks.push(tokio::spawn(runner(shard, bot.clone())));
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

pub fn bot_presence(activity: String) -> UpdatePresencePayload {
    UpdatePresencePayload {
        activities: vec![
            MinimalActivity {
                kind: ActivityType::Playing,
                name: activity,
                url: None,
            }
            .into(),
        ],
        afk: false,
        since: None,
        status: Status::Online,
    }
}

#[cfg(test)]
mod tests {

    use twilight_gateway::{Event, EventTypeFlags, StreamExt as _};

    use crate::config_handler::get_testing_config;

    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn start_2nd_bot_with_activity() {
        let config = get_testing_config().unwrap();
        let (_, shards) = boot_shards(config).await.unwrap();

        let mut senders = Vec::with_capacity(shards.len());
        let mut tasks = Vec::with_capacity(shards.len());
        for shard in shards {
            senders.push(shard.sender());
            tasks.push(tokio::spawn(async move {
                let mut shard = shard;
                while let Some(item) = shard.next_event(EventTypeFlags::all()).await {
                    let _ = match item {
                        Ok(Event::GatewayClose(_)) if SHUTDOWN.load(Ordering::Relaxed) => break,
                        Err(source) => {
                            tracing::error!(?source, "error receiving event");
                            continue;
                        }
                        _ => {
                            continue;
                        }
                    };
                }
            }));
        }

        signal::ctrl_c().await.unwrap();
        SHUTDOWN.store(true, Ordering::Relaxed);
        for sender in senders {
            // Ignore error if shard's already shutdown.
            _ = sender.close(CloseFrame::NORMAL);
        }

        for jh in tasks {
            _ = jh.await;
        }
    }
}

// TODO: Add cleanup slash command,
