use crate::{event_handler::DEBOUNCE_DELAY, rules_handler::GuildRules};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};
use tokio::{sync::Mutex, task::JoinHandle, time::sleep};
use twilight_cache_inmemory::InMemoryCache;
use twilight_http::Client;
use twilight_model::{
    gateway::presence::{Activity, ActivityType},
    id::{
        Id,
        marker::{GuildMarker, RoleMarker, UserMarker},
    },
};

pub struct RolesToChange {
    pub roles_to_add: BTreeSet<Id<RoleMarker>>,
    pub roles_to_remove: BTreeSet<Id<RoleMarker>>,
}

pub fn roles_for_activity(
    roles_rules: Arc<BTreeMap<u64, GuildRules>>,
    guild_id: Id<GuildMarker>,
    user_roles: BTreeSet<Id<RoleMarker>>,
    user_activities: BTreeSet<String>,
) -> Option<RolesToChange> {
    let guild_rules = match roles_rules.get(&guild_id.get()) {
        Some(guild_rules) => guild_rules,
        None => return None,
    };
    let managed_roles: BTreeSet<u64> = guild_rules.all_rules().iter().map(|r| r.role_id).collect();

    let rules_to_assign = guild_rules.matching_rules(user_activities);

    let roles_ids_to_assign: BTreeSet<u64> =
        rules_to_assign.iter().map(|rule| rule.role_id).collect();

    let user_roles: BTreeSet<u64> = user_roles
        .iter()
        .map(|role_id| role_id.get())
        .filter(|r| managed_roles.contains(r))
        .collect();

    let roles_to_add = roles_ids_to_assign
        .difference(&user_roles)
        .map(|id| Id::new(*id))
        .collect();
    let roles_to_remove = user_roles
        .difference(&roles_ids_to_assign)
        .map(|id| Id::new(*id))
        .collect();

    Some(RolesToChange {
        roles_to_add,
        roles_to_remove,
    })
}

pub async fn update_roles_by_activity(
    http_client: Arc<Client>,
    cache: Arc<InMemoryCache>,
    roles_rules: Arc<BTreeMap<u64, GuildRules>>,
    guild_id: Id<GuildMarker>,
    user_id: Id<UserMarker>,
    user_activities: BTreeSet<String>,
) {
    let user_roles: BTreeSet<Id<RoleMarker>> = match cache.member(guild_id, user_id) {
        Some(member) => member.roles().iter().cloned().collect(),
        None => {
            tracing::error!("Member not found in cache for user {user_id:?}");
            return;
        }
    };

    let RolesToChange {
        roles_to_add,
        roles_to_remove,
    } = match roles_for_activity(roles_rules, guild_id, user_roles, user_activities) {
        Some(x) => x,
        None => return,
    };

    for role_id in roles_to_add {
        tracing::warn!("Assigning Role {role_id:?} to {user_id:?} in {guild_id:?}");
        let r = http_client
            .add_guild_member_role(guild_id, user_id, role_id)
            .await;

        match r {
            Err(e) => tracing::error!(?e, "Couldn't add role"),
            Ok(_) => (),
        };
    }

    for role_id in roles_to_remove {
        tracing::warn!("Removing Role {role_id:?} to {user_id:?} in {guild_id:?}");
        let r = http_client
            .remove_guild_member_role(guild_id, user_id, role_id)
            .await;

        match r {
            Err(e) => tracing::error!(?e, "Couldn't remove role"),
            Ok(_) => (),
        };
    }
}

pub fn user_activities_from_presence<'a, T: Iterator<Item = &'a Activity>>(
    activities: T,
) -> BTreeSet<String> {
    activities
        .filter(|activity| activity.kind == ActivityType::Playing)
        .map(|activity| activity.name.to_string())
        .collect()
}

/// the actual logic to change roles for users based on presence
pub async fn handle_presence_update(
    http_client: Arc<Client>,
    rules: Arc<BTreeMap<u64, GuildRules>>,
    cache: Arc<InMemoryCache>,
    presence_update_tasks: Arc<Mutex<HashMap<(Id<GuildMarker>, Id<UserMarker>), JoinHandle<()>>>>,
    guild_id: Id<GuildMarker>,
    user_id: Id<UserMarker>,
    user_activities: BTreeSet<String>,
) {
    // Cancel existing task if exists
    let key = (guild_id, user_id);
    let mut tasks = presence_update_tasks.lock().await;
    if let Some(task) = tasks.remove(&key) {
        task.abort();
    }
    let future = update_roles_by_activity(
        http_client.clone(),
        cache.clone(),
        rules.clone(),
        guild_id,
        user_id,
        user_activities,
    );
    let task_handle = tokio::spawn(async {
        sleep(DEBOUNCE_DELAY).await;
        future.await;
    });

    tasks.insert(key, task_handle);
}
