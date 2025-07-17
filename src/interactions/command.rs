use anyhow::{Context, Result};
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::RwLock;
use twilight_http::Client;
use twilight_interactions::command::{
    CommandModel, CommandOption, CreateCommand, CreateOption, DescLocalizations, ResolvedUser,
};
use twilight_model::{
    application::interaction::{Interaction, application_command::CommandData},
    channel::message::{Embed, embed::EmbedField},
    guild::Role,
    http::interaction::{InteractionResponse, InteractionResponseData, InteractionResponseType},
    id::Id,
};
use twilight_util::builder::{
    InteractionResponseDataBuilder,
    embed::{EmbedBuilder, EmbedFieldBuilder, ImageSource},
};

use crate::{
    config_handler::GithubConfig,
    rules_handler::{self, GuildRules, RoleType},
};

#[derive(CommandModel, CreateCommand, Debug)]
#[command(name = "xkcd", desc_localizations = "xkcd_desc")]
pub enum XkcdCommand {
    #[command(name = "number")]
    Number(XkcdNumberCommand),
}

fn xkcd_desc() -> DescLocalizations {
    DescLocalizations::new("Explore xkcd comics", [("fr", "Explorer les comics xkcd")])
}

impl XkcdCommand {
    /// Handle incoming `/xkcd` commands.
    pub async fn handle(
        interaction: &Interaction,
        data: CommandData,
        client: &Client,
    ) -> Result<Option<InteractionResponseData>> {
        // Parse the command data into a structure using twilight-interactions.
        let command =
            XkcdCommand::from_interaction(data.into()).context("failed to parse command data")?;

        // Call the appropriate subcommand.
        match command {
            XkcdCommand::Number(command) => command.run(interaction, client).await,
        };

        Ok(None)
    }
}

#[derive(CommandModel, CreateCommand, Debug)]
#[command(name = "number", desc_localizations = "xkcd_number_desc")]
pub struct XkcdNumberCommand {
    /// Comic number
    #[command(min_value = 1, desc_localizations = "xkcd_number_arg_desc")]
    pub number: i64,
}

fn xkcd_number_desc() -> DescLocalizations {
    DescLocalizations::new(
        "Show a specific xkcd comic",
        [("fr", "Afficher un comic xkcd spécifique")],
    )
}

fn xkcd_number_arg_desc() -> DescLocalizations {
    DescLocalizations::new("Comic number", [("fr", "Numéro du comic")])
}

impl XkcdNumberCommand {
    /// Run the `/xkcd number <num>` command.
    pub async fn run(&self, interaction: &Interaction, client: &Client) -> anyhow::Result<()> {
        let mut data = InteractionResponseDataBuilder::new();
        if self.number == 1 {
            data = data.embeds([crate_embed()?]);
        } else {
            data = data.content(format!("No comic found for number {}", self.number));
        }

        let client = client.interaction(interaction.application_id);
        let response = InteractionResponse {
            kind: InteractionResponseType::ChannelMessageWithSource,
            data: Some(data.build()),
        };

        client
            .create_response(interaction.id, &interaction.token, &response)
            .await?;

        Ok(())
    }
}

/// Create a Discord embed for a comic
fn crate_embed() -> anyhow::Result<Embed> {
    let image = ImageSource::url(&"https://i.imgur.com/hwODj8F.jpeg".to_string())?;
    let title = format!("Embed Title");

    let embed = EmbedBuilder::new()
        .color(0x2f3136) // Dark theme color, render a "transparent" background
        .title(title)
        .url("https://imgur.com/gallery/raccoon-7CTFQre#/t/racoon")
        .image(image)
        .build();

    Ok(embed)
}

#[derive(CommandModel, CreateCommand, Debug)]
#[command(name = "list-guild-rules", desc = "List Rules for Guild")]
pub struct GuildRulesList;

impl GuildRulesList {
    pub async fn handle(
        interaction: &Interaction,
        data: CommandData,
        client: &Client,
        rules: &Arc<RwLock<BTreeMap<u64, GuildRules>>>,
    ) -> Result<Option<InteractionResponseData>> {
        // Call the appropriate subcommand.
        GuildRulesList.run(interaction, client, rules).await?;
        Ok(None)
    }

    pub async fn run(
        &self,
        interaction: &Interaction,
        client: &Client,
        rules: &Arc<RwLock<BTreeMap<u64, GuildRules>>>,
    ) -> anyhow::Result<()> {
        let guild_rules = {
            let rules_reader = rules.read().await;
            rules_reader
                .get(&interaction.guild_id.unwrap_or(Id::new(1)).get())
                .cloned()
        };

        let title = format!("Guild Rules");

        let fields = match guild_rules {
            Some(guild_rules) => guild_rules.clone().into(),
            None => vec![],
        };

        let mut embed = EmbedBuilder::new()
            .color(0x2f3136) // Dark theme color, render a "transparent" background
            .title(title)
            .build();

        embed.fields = fields;

        let data = InteractionResponseDataBuilder::new()
            .embeds([embed])
            .build();

        let response = InteractionResponse {
            kind: InteractionResponseType::ChannelMessageWithSource,
            data: Some(data),
        };

        client
            .interaction(interaction.application_id)
            .create_response(interaction.id, &interaction.token, &response)
            .await?;

        Ok(())
    }
}

#[derive(CommandModel, CreateCommand, Debug)]
#[command(name = "storage", desc = "Save/Load to Storage, BotFather only")]
pub struct StorageCommand {
    #[command(desc = "Storage Command")]
    pub storage_command: StorageCommandOptions,
}

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

impl StorageCommand {
    pub async fn handle(
        interaction: &Interaction,
        data: CommandData,
        client: &Client,
        rules: &Arc<RwLock<BTreeMap<u64, GuildRules>>>,
        github_config: Option<&GithubConfig>,
    ) -> Result<Option<InteractionResponseData>> {
        let command = StorageCommand::from_interaction(data.into())
            .context("failed to parse command data")?;

        match command.storage_command {
            StorageCommandOptions::SaveToFile => {
                let rules = rules.read().await;
                rules_handler::save_db_to_file(&rules);
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
                Ok(None)
            }
            StorageCommandOptions::LoadFromFile => {
                let mut rules_writer = rules.write().await;
                let rules = rules_handler::load_db_from_file()?;
                *rules_writer = rules;
                Ok(None)
            }
            StorageCommandOptions::LoadFromGithub => {
                let mut rules_writer = rules.write().await;
                let rules = rules_handler::load_rules_from_github(
                    github_config.ok_or(anyhow::anyhow!("No github config"))?,
                )
                .await?;
                *rules_writer = rules;
                Ok(None)
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
        client: &Client,
        rules: &Arc<RwLock<BTreeMap<u64, GuildRules>>>,
    ) -> Result<Option<InteractionResponseData>> {
        // Parse the command data into a structure using twilight-interactions.
        let command =
            ManageCommand::from_interaction(data.into()).context("failed to parse command data")?;

        // Call the appropriate subcommand.
        match command {
            // ManageCommand::Add(command) => command.run(interaction, client, rules).await,
            // ManageCommand::Remove(command) => command.run(interaction, client, rules).await,
            // ManageCommand::Edit(command) => command.run(interaction, client, rules).await,
            ManageCommand::List(command) => command.run(interaction, client, rules).await,
            _ => Ok(()),
        };

        Ok(None)
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

#[derive(CommandModel, CreateCommand, Debug)]
#[command(name = "remove", desc = "Remove Role Rule, Stops assigning the role")]
pub struct RemoveRoleRule {
    #[command(desc = "Role Tag")]
    pub role_tag: Role,
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
        client: &Client,
        rules: &Arc<RwLock<BTreeMap<u64, GuildRules>>>,
    ) -> anyhow::Result<()> {
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
            None => rules
                .read()
                .await
                .get(&guild_id)
                .ok_or(anyhow::anyhow!("No guild rules"))?
                .clone()
                .into(),
        };

        let title = format!("Guild Rules");

        let mut embed = EmbedBuilder::new()
            .color(0x2f3136) // Dark theme color, render a "transparent" background
            .title(title)
            .build();

        embed.fields = embed_fields;

        let client = client.interaction(interaction.application_id);
        let data = InteractionResponseDataBuilder::new()
            .embeds([embed])
            .build();

        let response = InteractionResponse {
            kind: InteractionResponseType::ChannelMessageWithSource,
            data: Some(data),
        };

        client
            .create_response(interaction.id, &interaction.token, &response)
            .await?;

        Ok(())
    }
}

#[derive(CommandModel, CreateCommand, Debug)]
#[command(name = "test-command", desc = "Test Command")]
pub struct TestCommand {
    #[command(desc = "Test User Option")]
    pub test_user: ResolvedUser,

    #[command(desc = "Test Role Option")]
    pub test_role: Role,
}

impl TestCommand {
    pub async fn handle(
        interaction: &Interaction,
        data: CommandData,
        client: &Client,
    ) -> Result<Option<InteractionResponseData>> {
        // Call the appropriate subcommand.
        let command =
            TestCommand::from_interaction(data.into()).context("failed to parse command data")?;

        // Call the appropriate subcommand.
        match command {
            command => command.run(interaction, client).await,
        };

        Ok(None)
    }

    pub async fn run(&self, interaction: &Interaction, client: &Client) -> anyhow::Result<()> {
        let title = format!("test Command");

        let mut embed = EmbedBuilder::new()
            .color(0x2f3136) // Dark theme color, render a "transparent" background
            .title(title)
            .field(EmbedFieldBuilder::new(
                "test_user",
                self.test_user.resolved.name.clone(),
            ))
            .field(EmbedFieldBuilder::new(
                "test_role",
                self.test_role.name.clone(),
            ))
            .build();

        let client = client.interaction(interaction.application_id);
        let data = InteractionResponseDataBuilder::new()
            .embeds([embed])
            .build();

        let response = InteractionResponse {
            kind: InteractionResponseType::ChannelMessageWithSource,
            data: Some(data),
        };

        client
            .create_response(interaction.id, &interaction.token, &response)
            .await?;

        Ok(())
    }
}
