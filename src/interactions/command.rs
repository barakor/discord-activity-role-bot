use crate::{
    config_handler::GithubConfig,
    rules_handler::{self, GuildRules, RoleType, Rule},
};
use anyhow::{Context, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use tokio::sync::RwLock;
use twilight_cache_inmemory::InMemoryCache;
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption};
use twilight_model::{
    application::interaction::{Interaction, application_command::CommandData},
    channel::message::embed::EmbedField,
    guild::Role,
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::{InteractionResponseDataBuilder, embed::EmbedBuilder};

#[derive(Debug, CommandOption, CreateOption)]
pub enum StorageCommandOptions {
    #[option(name = "Save to File", value = "save-to-file")]
    SaveToFile,

    #[option(name = "Load from File", value = "load-from-file")]
    LoadFromFile,

    #[option(name = "Save to Github", value = "save-to-github")]
    SaveToGithub,

    #[option(name = "Load from Github", value = "load-from-github")]
    LoadFromGithub,
}

#[derive(CommandModel, CreateCommand, Debug)]
#[command(name = "storage", desc = "Save/Load to Storage, BotFather only")]
pub struct StorageCommand {
    #[command(desc = "Storage Command")]
    pub storage_command: StorageCommandOptions,
}

impl StorageCommand {
    pub async fn handle(
        data: CommandData,
        rules: &Arc<RwLock<BTreeMap<u64, GuildRules>>>,
        github_config: Option<&GithubConfig>,
    ) -> Result<Option<InteractionResponseData>> {
        let command = StorageCommand::from_interaction(data.into())
            .context("failed to parse command data")?;

        match command.storage_command {
            StorageCommandOptions::SaveToFile => {
                let rules = rules.read().await;
                rules_handler::save_db_to_file(&rules)?;
                Ok(Some(InteractionResponseData {
                    content: Some("Rules saved to file".to_string()),
                    ..Default::default()
                }))
            }
            StorageCommandOptions::SaveToGithub => {
                let rules = rules.read().await;
                rules_handler::save_db_to_github(
                    &rules,
                    github_config.ok_or(anyhow::anyhow!("No github config"))?,
                )
                .await?;
                Ok(Some(InteractionResponseData {
                    content: Some("Rules saved to github".to_string()),
                    ..Default::default()
                }))
            }
            StorageCommandOptions::LoadFromFile => {
                let mut rules_writer = rules.write().await;
                let rules = rules_handler::load_db_from_file()?;
                *rules_writer = rules;
                Ok(Some(InteractionResponseData {
                    content: Some("Rules loaded from file".to_string()),
                    ..Default::default()
                }))
            }
            StorageCommandOptions::LoadFromGithub => {
                let mut rules_writer = rules.write().await;
                let rules = rules_handler::load_rules_from_github(
                    github_config.ok_or(anyhow::anyhow!("No github config"))?,
                )
                .await?;
                *rules_writer = rules;
                Ok(Some(InteractionResponseData {
                    content: Some("Rules loaded from github".to_string()),
                    ..Default::default()
                }))
            }
        }
    }
}

#[derive(CommandModel, CreateCommand, Debug)]
#[command(name = "manage", desc = "Manage Guild Roles Rules")]
pub enum ManageCommand {
    #[command(name = "add")]
    Add(AddRoleRule),

    #[command(name = "remove")]
    Remove(RemoveRoleRule),

    #[command(name = "edit")]
    Edit(EditRoleRule),

    #[command(name = "list")]
    List(ListRoleRule),
}

impl ManageCommand {
    /// Handle incoming `/xkcd` commands.
    pub async fn handle(
        interaction: &Interaction,
        data: CommandData,
        cache: &Arc<InMemoryCache>,
        rules: &Arc<RwLock<BTreeMap<u64, GuildRules>>>,
    ) -> Result<Option<InteractionResponseData>> {
        // Parse the command data into a structure using twilight-interactions.
        let command =
            ManageCommand::from_interaction(data.into()).context("failed to parse command data")?;

        // Call the appropriate subcommand.
        match command {
            ManageCommand::Add(command) => command.run(cache, interaction, rules).await,
            ManageCommand::Remove(command) => command.run(interaction, rules).await,
            // ManageCommand::Edit(command) => command.run(interaction, client, rules).await,
            ManageCommand::List(command) => command.run(interaction, rules).await,
            _ => Ok(None),
        }
    }
}

#[derive(CommandModel, CreateCommand, Debug)]
#[command(name = "add", desc = "Add Role Rule")]
pub struct AddRoleRule {
    #[command(desc = "Role Tag")]
    pub role_tag: Role,

    #[command(desc = "Type")]
    pub role_type: RoleType,

    #[command(desc = "Comment")]
    pub comment: Option<String>,
}

impl AddRoleRule {
    pub async fn run(
        &self,
        cache: &Arc<InMemoryCache>,
        interaction: &Interaction,
        rules: &Arc<RwLock<BTreeMap<u64, GuildRules>>>,
    ) -> Result<Option<InteractionResponseData>> {
        let guild_id = interaction.guild_id.ok_or(anyhow::anyhow!("No guild id"))?;

        let mut rules_writer = rules.write().await;
        rules_writer
            .get_mut(&guild_id.into())
            .ok_or(anyhow::anyhow!("No guild rules"))?
            .add_rule(Rule {
                guild_id: guild_id.into(),
                guild_name: cache
                    .guild(guild_id)
                    .ok_or(anyhow::anyhow!("No guild"))?
                    .name()
                    .to_string(),
                role_id: self.role_tag.id.get(),
                role_name: self.role_tag.name.clone(),
                role_type: self.role_type.clone(),
                activities: BTreeSet::new(),
                comments: self.comment.clone().unwrap_or("".to_string()),
            })?;

        Ok(Some(InteractionResponseData {
            content: Some("Rule added".to_string()),
            ..Default::default()
        }))
    }
}

#[derive(CommandModel, CreateCommand, Debug)]
#[command(name = "remove", desc = "Remove Role Rule, Stops assigning the role")]
pub struct RemoveRoleRule {
    #[command(desc = "Role Tag")]
    pub role_tag: Role,
}

impl RemoveRoleRule {
    pub async fn run(
        &self,
        interaction: &Interaction,
        rules: &Arc<RwLock<BTreeMap<u64, GuildRules>>>,
    ) -> Result<Option<InteractionResponseData>> {
        let guild_id = interaction
            .guild_id
            .ok_or(anyhow::anyhow!("No guild id"))?
            .get();

        let mut rules_writer = rules.write().await;
        rules_writer
            .get_mut(&guild_id)
            .ok_or(anyhow::anyhow!("No guild rules"))?
            .remove_rule(self.role_tag.id.get())?;

        Ok(Some(InteractionResponseData {
            content: Some("Rule removed".to_string()),
            ..Default::default()
        }))
    }
}

#[derive(CommandModel, CreateCommand, Debug)]
#[command(name = "edit", desc = "Edit Role Rule")]
pub struct EditRoleRule {
    #[command(desc = "Role Tag")]
    pub role_tag: Role,

    #[command(desc = "Add Activities, `;` seperated")]
    pub add_activities: Option<String>,

    #[command(desc = "Remove Activities, `;` seperated")]
    pub remove_activities: Option<String>,
}

#[derive(CommandModel, CreateCommand, Debug)]
#[command(
    name = "list",
    desc = "Shows Role Rule, if no role tag is provided, shows all rules"
)]
pub struct ListRoleRule {
    #[command(desc = "Role Tag")]
    pub role_tag: Option<Role>,
}

impl ListRoleRule {
    pub async fn run(
        &self,
        interaction: &Interaction,
        rules: &Arc<RwLock<BTreeMap<u64, GuildRules>>>,
    ) -> Result<Option<InteractionResponseData>> {
        let guild_id = interaction
            .guild_id
            .ok_or(anyhow::anyhow!("No guild id"))?
            .get();

        let embed_fields: Vec<EmbedField> = match &self.role_tag {
            Some(role_tag) => {
                let role_id = role_tag.id.get();
                let rules_reader = rules.read().await;
                let rule = rules_reader
                    .get(&guild_id)
                    .ok_or(anyhow::anyhow!("No guild rules"))?
                    .get_rule(role_id)
                    .ok_or(anyhow::anyhow!("No rule found"))?;
                vec![rule.clone().into()]
            }
            None => {
                let rules_reader = rules.read().await;
                let rules = rules_reader
                    .get(&guild_id)
                    .ok_or(anyhow::anyhow!("No guild rules"))?
                    .clone();
                rules.into()
            }
        };

        let title = format!("Guild Rules");

        let mut embed = EmbedBuilder::new()
            .color(0x2f3136) // Dark theme color, render a "transparent" background
            .title(title)
            .build();

        embed.fields = embed_fields;

        let response = InteractionResponseDataBuilder::new()
            .embeds([embed])
            .build();

        Ok(Some(response))
    }
}
