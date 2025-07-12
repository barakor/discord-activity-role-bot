mod config_handler;
mod event_handler;
mod rules_handler;

use anyhow::Result;
use config_handler::get_config;
use event_handler::runner;
use std::sync::{Arc, atomic::Ordering};
use tokio::signal;
use twilight_gateway::{CloseFrame, Config, Intents};
use twilight_http::Client;

use crate::event_handler::{Bot, SHUTDOWN};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let config = get_config()?;
    let token = config.discord_token;
    // Initialize the tracing subscriber.

    let intents = Intents::GUILD_PRESENCES | Intents::GUILDS;
    let client = Client::new(token.clone());
    let config = Config::new(token, intents);

    let shards =
        twilight_gateway::create_recommended(&client, config, |_, builder| builder.build()).await?;
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
