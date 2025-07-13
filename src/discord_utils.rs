use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use tokio::sync::Mutex;
use twilight_cache_inmemory::InMemoryCache;
use twilight_http::Client;
use twilight_model::{
    gateway::{
        payload::incoming::PresenceUpdate,
        presence::{self, ClientStatus, Presence, Status},
    },
    id::{
        Id,
        marker::{GuildMarker, UserMarker},
    },
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
    cache: Arc<InMemoryCache>,
    presence_queue: Arc<Mutex<HashMap<(Id<GuildMarker>, Id<UserMarker>), PresenceUpdate>>>,
    guild_id: Id<GuildMarker>,
) -> Result<()> {
    let guild_members = get_all_guild_members(&http_client, guild_id).await?;
    let mut queue = presence_queue.lock().await;

    for user_id in guild_members {
        let (status, activities) = match cache.presence(guild_id, user_id) {
            Some(presence) => (
                presence.status().clone(),
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
