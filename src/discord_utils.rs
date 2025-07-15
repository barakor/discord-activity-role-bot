use crate::{
    events::{handle_presence_update, user_activities_from_presence},
    rules_handler::GuildRules,
};
use anyhow::Result;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};
use tokio::{sync::Mutex, task::JoinHandle};
use twilight_cache_inmemory::InMemoryCache;
use twilight_http::Client;
use twilight_model::id::{
    Id,
    marker::{GuildMarker, UserMarker},
};

pub async fn get_all_guild_members(
    client: &Client,
    guild_id: Id<GuildMarker>,
) -> Result<Vec<Id<UserMarker>>> {
    let mut after: Option<Id<UserMarker>> = None;
    let mut user_ids = Vec::new();

    loop {
        let members_result = client
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

pub async fn purge_guild_roles(
    http_client: Arc<Client>,
    rules: Arc<BTreeMap<u64, GuildRules>>,
    cache: Arc<InMemoryCache>,
    presence_update_tasks: Arc<Mutex<HashMap<(Id<GuildMarker>, Id<UserMarker>), JoinHandle<()>>>>,
    guild_id: Id<GuildMarker>,
) -> Result<()> {
    let guild_members = get_all_guild_members(&http_client, guild_id).await?;

    for user_id in guild_members {
        let user_activities = match cache.presence(guild_id, user_id) {
            Some(presence) => user_activities_from_presence(presence.activities().iter()),
            None => BTreeSet::new(),
        };

        let future = handle_presence_update(
            http_client.clone(),
            rules.clone(),
            cache.clone(),
            presence_update_tasks.clone(),
            guild_id,
            user_id,
            user_activities,
        );
        tokio::spawn(future);
    }
    Ok({})
}
