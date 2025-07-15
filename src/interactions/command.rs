use std::collections::BTreeMap;

use anyhow::Context;
use twilight_http::Client;
use twilight_interactions::command::{CommandModel, CreateCommand, DescLocalizations};
use twilight_model::{
    application::interaction::{Interaction, application_command::CommandData},
    channel::message::Embed,
    http::interaction::{InteractionResponse, InteractionResponseType},
    id::Id,
};
use twilight_util::builder::{
    InteractionResponseDataBuilder,
    embed::{EmbedBuilder, EmbedFieldBuilder, ImageSource},
};

use crate::rules_handler::GuildRules;

#[derive(CommandModel, CreateCommand, Debug)]
#[command(name = "xkcd", desc_localizations = "xkcd_desc")]
pub enum XkcdCommand {
    #[command(name = "latest")]
    Latest(XkcdLatestCommand),
    #[command(name = "number")]
    Number(XkcdNumberCommand),
    #[command(name = "random")]
    Random(XkcdRandomCommand),
}

fn xkcd_desc() -> DescLocalizations {
    DescLocalizations::new("Explore xkcd comics", [("fr", "Explorer les comics xkcd")])
}

impl XkcdCommand {
    /// Handle incoming `/xkcd` commands.
    pub async fn handle(
        interaction: Interaction,
        data: CommandData,
        client: &Client,
    ) -> anyhow::Result<()> {
        // Parse the command data into a structure using twilight-interactions.
        let command =
            XkcdCommand::from_interaction(data.into()).context("failed to parse command data")?;

        // Call the appropriate subcommand.
        match command {
            XkcdCommand::Latest(command) => command.run(interaction, client).await,
            XkcdCommand::Number(command) => command.run(interaction, client).await,
            XkcdCommand::Random(command) => command.run(interaction, client).await,
        }
    }
}

#[derive(CommandModel, CreateCommand, Debug)]
#[command(name = "latest", desc_localizations = "xkcd_latest_desc")]
pub struct XkcdLatestCommand;

fn xkcd_latest_desc() -> DescLocalizations {
    DescLocalizations::new(
        "Show the latest xkcd comic",
        [("fr", "Afficher le dernier comic xkcd")],
    )
}

impl XkcdLatestCommand {
    /// Run the `/xkcd latest` command.
    pub async fn run(&self, interaction: Interaction, client: &Client) -> anyhow::Result<()> {
        let embed = crate_embed()?;

        // Respond to the interaction with an embed.
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
    pub async fn run(&self, interaction: Interaction, client: &Client) -> anyhow::Result<()> {
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

#[derive(CommandModel, CreateCommand, Debug)]
#[command(name = "random", desc_localizations = "xkcd_random_desc")]
pub struct XkcdRandomCommand;

fn xkcd_random_desc() -> DescLocalizations {
    DescLocalizations::new(
        "Show a random xkcd comic",
        [("fr", "Afficher un comic xkcd aléatoire")],
    )
}

impl XkcdRandomCommand {
    /// Run the `/xkcd random` command.
    pub async fn run(&self, interaction: Interaction, client: &Client) -> anyhow::Result<()> {
        let embed = crate_embed()?;

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
        interaction: Interaction,
        data: CommandData,
        client: &Client,
        rules: &BTreeMap<u64, GuildRules>,
    ) -> anyhow::Result<()> {
        // Call the appropriate subcommand.
        GuildRulesList.run(interaction, client, rules).await
    }

    pub async fn run(
        &self,
        interaction: Interaction,
        client: &Client,
        rules: &BTreeMap<u64, GuildRules>,
    ) -> anyhow::Result<()> {
        let guild_rules = rules.get(&interaction.guild_id.unwrap_or(Id::new(1)).get());

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
