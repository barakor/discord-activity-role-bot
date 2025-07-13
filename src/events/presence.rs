use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use governor::DefaultDirectRateLimiter;
use tokio::time::sleep;
use twilight_cache_inmemory::InMemoryCache;
use twilight_http::Client;
use twilight_model::id::{
    Id,
    marker::{GuildMarker, RoleMarker, UserMarker},
};

use crate::{event_handler::DEBOUNCE_DELAY, rules_handler::GuildRules};

pub async fn update_roles_by_activity(
    http_client: Arc<Client>,
    limiter: Arc<DefaultDirectRateLimiter>,
    cache: Arc<InMemoryCache>,
    roles_rules: BTreeMap<u64, GuildRules>,
    guild_id: Id<GuildMarker>,
    user_id: Id<UserMarker>,
    user_activities: BTreeSet<String>,
) {
    sleep(DEBOUNCE_DELAY).await;

    let guild_rules = match roles_rules.get(&guild_id.get()) {
        Some(guild_rules) => guild_rules,
        None => return,
    };
    let managed_roles: BTreeSet<u64> = guild_rules.all_rules().iter().map(|r| r.role_id).collect();

    let rules_to_assign = guild_rules.matching_rules(user_activities);

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
}
