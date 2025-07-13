use anyhow::Result;
use governor::DefaultDirectRateLimiter;
use std::sync::Arc;
use twilight_cache_inmemory::{CacheableRole, InMemoryCache};
use twilight_http::{Client, request::AuditLogReason};
use twilight_model::id::{Id, marker::GuildMarker};

pub const LEZYES_ID: u64 = 88533822521507840;

pub async fn easter(
    http_client: Arc<Client>,
    limiter: Arc<DefaultDirectRateLimiter>,
    cache: Arc<InMemoryCache>,
    guild_id: Id<GuildMarker>,
) -> Result<()> {
    let role_name = "Lazy Null".to_string();
    let reason = "Heil the king of nothing and master of null".to_string();
    let role_color = 15877376;

    let lazy_null_roles = match cache.guild_roles(guild_id) {
        None => Vec::new(),
        Some(roles) => roles
            .iter()
            .filter_map(|role_id| match cache.role(*role_id) {
                Some(role) if role.name.eq(&role_name) => Some(role.clone()),

                _ => None,
            })
            .collect(),
    };
    let lazy_null_role_id = match lazy_null_roles.first() {
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

    limiter.until_ready().await;
    http_client
        .add_guild_member_role(guild_id, Id::new(LEZYES_ID), lazy_null_role_id)
        .reason(&reason)
        .await?;

    Ok({})
}
