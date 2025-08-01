use anyhow::Result;
use std::sync::Arc;
use twilight_cache_inmemory::CacheableRole;
use twilight_http::{Client, request::AuditLogReason};
use twilight_model::id::{Id, marker::GuildMarker};

pub const LEZYES_ID: u64 = 88533822521507840;

pub async fn easter(http_client: Arc<Client>, guild_id: Id<GuildMarker>) -> Result<()> {
    let role_name = "Lazy Null".to_string();
    let reason = "Heil the king of nothing and master of null".to_string();
    let role_color = 15877376;

    // use this instead of the cache
    let roles = http_client.roles(guild_id).await?.model().await?;
    let lazy_null_role = roles.iter().find(|role| role.name.eq(&role_name));

    let lazy_null_role_id = match lazy_null_role {
        Some(role) => role.id(),
        None => http_client
            .create_role(guild_id)
            .color(role_color)
            .name(&role_name)
            .await?
            .model()
            .await?
            .id(),
    };

    http_client
        .add_guild_member_role(guild_id, Id::new(LEZYES_ID), lazy_null_role_id)
        .reason(&reason)
        .await?;

    Ok({})
}
