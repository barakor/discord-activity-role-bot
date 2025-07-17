use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{BufReader, Read},
    sync::Arc,
};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use twilight_interactions::command::{CommandOption, CreateOption};
use twilight_model::{channel::message::embed::EmbedField, guild::Role};

use crate::{
    config_handler::GithubConfig,
    github_handler::{get_bytes_from_github, upload_bytes_to_github},
};
use bytes::Bytes;

use std::sync::atomic::{AtomicBool, Ordering};

static RULES_HANDLER_STARTED: AtomicBool = AtomicBool::new(false);

/// Mark the rules handler as started or not.
pub fn set_rules_handler_started(started: bool) {
    RULES_HANDLER_STARTED.store(started, Ordering::SeqCst);
}

/// Check if the rules handler has been started.
pub fn is_rules_handler_started() -> bool {
    RULES_HANDLER_STARTED.load(Ordering::SeqCst)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, CommandOption, CreateOption)]
pub enum RoleType {
    #[option(name = "Activity Based Role", value = "named-activity")]
    NamedActivity,

    #[option(name = "Otherwise Role", value = "else")]
    Else,
}

impl RoleType {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "named-activity" => Some(RoleType::NamedActivity),
            "else" => Some(RoleType::Else),
            _ => None,
        }
    }

    fn to_str(&self) -> &str {
        match self {
            RoleType::NamedActivity => "named-activity",
            RoleType::Else => "else",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Rule {
    pub guild_id: u64,
    pub guild_name: String,
    pub role_id: u64,
    pub role_name: String,
    pub role_type: RoleType,
    pub activities: BTreeSet<String>,
    pub comments: String,
}

impl Into<EmbedField> for Rule {
    fn into(self) -> EmbedField {
        let activities: Vec<String> = self.activities.iter().map(|x| x.to_string()).collect();
        let rule_value = if activities.is_empty() {
            "Default Role".to_string()
        } else {
            activities.join(", ")
        };
        EmbedField {
            inline: false,
            name: self.role_name,
            value: rule_value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GuildRules {
    activities_rules: BTreeMap<u64, Rule>,
    default_rule: Option<Rule>,
}

impl GuildRules {
    pub fn new() -> Self {
        GuildRules {
            default_rule: None,
            activities_rules: BTreeMap::new(),
        }
    }

    fn default_rules(&self) -> BTreeSet<Rule> {
        match &self.default_rule {
            None => BTreeSet::new(),
            Some(rule) => BTreeSet::from_iter([rule.clone()]),
        }
    }

    pub fn all_rules(&self) -> BTreeSet<Rule> {
        self.default_rules()
            .union(&self.activities_rules.values().cloned().collect())
            .cloned()
            .collect()
    }

    pub fn matching_rules(&self, user_activities: BTreeSet<String>) -> BTreeSet<Rule> {
        if user_activities.is_empty() {
            return BTreeSet::new();
        };

        let activity_rules: BTreeSet<Rule> = self
            .activities_rules
            .values()
            .filter(|rule| {
                rule.activities.iter().any(|rule_activity| {
                    user_activities.iter().any(|user_activity| {
                        user_activity
                            .to_lowercase()
                            .contains(&rule_activity.to_lowercase().to_string())
                    })
                })
            })
            .cloned()
            .collect();
        match activity_rules.is_empty() {
            false => activity_rules,
            true => self.default_rules(),
        }
    }

    pub fn get_rule(&self, role_id: u64) -> Option<&Rule> {
        match &self.default_rule {
            Some(rule) if rule.role_id == role_id => Some(rule),
            _ => self.activities_rules.get(&role_id),
        }
    }

    pub fn get_rule_mut(&mut self, role_id: u64) -> Option<&mut Rule> {
        match &mut self.default_rule {
            Some(rule) if rule.role_id == role_id => Some(rule),
            _ => self.activities_rules.get_mut(&role_id),
        }
    }

    pub async fn add_rule(&mut self, rule: Rule) -> Result<()> {
        match rule.role_type {
            RoleType::NamedActivity => match self.activities_rules.contains_key(&rule.role_id) {
                true => Err(anyhow::anyhow!("Rule already exists")),
                false => {
                    self.activities_rules.insert(rule.role_id, rule);
                    Ok(())
                }
            },
            RoleType::Else => match &self.default_rule {
                Some(_) => Err(anyhow::anyhow!("Default rule already exists")),
                None => {
                    self.default_rule = Some(rule);
                    Ok(())
                }
            },
        }
    }

    pub async fn remove_rule(&mut self, role_id: u64) -> Result<()> {
        if self.activities_rules.contains_key(&role_id) {
            self.activities_rules.remove(&role_id);
            Ok(())
        } else if let Some(rule) = &self.default_rule
            && rule.role_id == role_id
        {
            self.default_rule = None;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Rule not found"))
        }
    }

    pub async fn edit_rule(&mut self, rule: Rule) -> Result<()> {
        if self.activities_rules.contains_key(&rule.role_id) {
            self.activities_rules.insert(rule.role_id, rule);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Rule not found"))
        }
    }
}

impl Into<Vec<EmbedField>> for GuildRules {
    fn into(self) -> Vec<EmbedField> {
        self.activities_rules
            .values()
            .map(|r| r.clone().into())
            .chain(self.default_rule.iter().map(|r| r.clone().into()))
            .collect()
    }
}

impl From<CsvRow> for Rule {
    fn from(row: CsvRow) -> Self {
        let guild_id = row.guild_id.parse().expect("Invalid guild_id");

        let role_type = RoleType::from_str(&row.role_type)
            .unwrap_or_else(|| panic!("Unknown role_type: {}", row.role_type));

        let activities = row
            .activity_names
            .split(';')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Rule {
            guild_id,
            guild_name: row.guild_name,
            role_id: row.role_id.parse().expect("Invalid role_id"),
            role_name: row.role_name,
            role_type,
            activities,
            comments: row.comments,
        }
    }
}

impl Into<CsvRow> for Rule {
    fn into(self) -> CsvRow {
        let mut activities: Vec<String> = self.activities.iter().cloned().collect();
        activities.sort();
        CsvRow {
            guild_id: self.guild_id.to_string(),
            guild_name: self.guild_name,
            role_id: self.role_id.to_string(),
            role_name: self.role_name,
            role_type: self.role_type.to_str().to_string(),
            activity_names: activities.join(";"),
            comments: self.comments,
        }
    }
}

impl Into<Vec<CsvRow>> for GuildRules {
    fn into(self) -> Vec<CsvRow> {
        let mut rows = match self.default_rule {
            Some(r) => vec![r.into()],
            None => vec![],
        };
        rows.extend(self.activities_rules.values().map(|r| r.clone().into()));
        rows
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct CsvRow {
    guild_id: String,
    guild_name: String,
    role_id: String,
    role_name: String,

    #[serde(rename = "type")]
    role_type: String,

    activity_names: String,
    comments: String,
}

pub async fn update_roles_names(
    rules: Arc<RwLock<BTreeMap<u64, GuildRules>>>,
    guild_roles: Vec<Role>,
    guild_id: u64,
) -> Result<()> {
    let mut wrtr = rules.write().await;
    let guild_rules = wrtr
        .get_mut(&guild_id)
        .ok_or(anyhow::anyhow!("No rules for guild"))?;

    guild_roles.iter().for_each(|guild_role| {
        let role_id = guild_role.id.into();

        // update rule's role name
        let rule = guild_rules.get_rule_mut(role_id);
        match rule {
            Some(rule) => rule.role_name = guild_role.name.to_string(),
            None => (),
        }
    });

    Ok(())
}

pub fn load_rules_from_buffer<R: Read>(reader: R) -> Result<BTreeMap<u64, GuildRules>> {
    let mut reader_buffer = csv::Reader::from_reader(reader);
    let mut rules = BTreeMap::new();

    for result in reader_buffer.deserialize() {
        let row: CsvRow = result?;
        let rule: Rule = row.into();

        let guild_rules = rules.entry(rule.guild_id).or_insert(GuildRules::new());

        match rule.role_type {
            RoleType::NamedActivity => {
                guild_rules.activities_rules.insert(rule.role_id, rule);
            }
            RoleType::Else => {
                guild_rules.default_rule = Some(rule);
            }
        }
    }

    Ok(rules)
}

pub fn load_rules_from_file(file_path: String) -> Result<BTreeMap<u64, GuildRules>> {
    let file = File::open(file_path)?;
    Ok(load_rules_from_buffer(BufReader::new(file))?)
}

pub fn load_db_from_file() -> Result<BTreeMap<u64, GuildRules>> {
    load_rules_from_file("db.csv".to_string())
}

pub async fn load_rules_from_github(
    github_config: &GithubConfig,
) -> Result<BTreeMap<u64, GuildRules>> {
    Ok(load_rules_from_buffer(
        get_bytes_from_github(
            &github_config.owner,
            &github_config.repo,
            "db.csv",
            &github_config.branch,
        )
        .await?
        .as_slice(),
    )?)
}

pub fn rules_to_csv_bytes(rules: &BTreeMap<u64, GuildRules>) -> Result<Vec<u8>> {
    let mut wtr = csv::Writer::from_writer(Vec::new());

    // Collect all rules from all guilds
    let mut all_csv_rows: Vec<CsvRow> = rules
        .values()
        .flat_map(|guild_rules| Into::<Vec<CsvRow>>::into(guild_rules.clone()))
        .collect();

    // Sort by guild_id then by role_name for consistent output
    all_csv_rows.sort_by(|a, b| match a.guild_id.cmp(&b.guild_id) {
        std::cmp::Ordering::Equal => a.role_id.cmp(&b.role_id),
        o => o,
    });

    // Write all rows
    for row in all_csv_rows {
        wtr.serialize(row)?;
    }

    wtr.flush()?;
    Ok(wtr.into_inner()?)
}

pub fn save_rules_to_file(rules: &BTreeMap<u64, GuildRules>, file_path: String) -> Result<()> {
    let csv_bytes = rules_to_csv_bytes(rules)?;
    std::fs::write(file_path, csv_bytes)?;
    Ok(())
}

pub fn save_db_to_file(rules: &BTreeMap<u64, GuildRules>) -> Result<()> {
    save_rules_to_file(rules, "db.csv".to_string())
}

pub async fn save_db_to_github(
    rules: &BTreeMap<u64, GuildRules>,
    github_config: &GithubConfig,
) -> Result<()> {
    let csv_bytes = rules_to_csv_bytes(rules)?;
    let bytes = Bytes::from(csv_bytes);

    upload_bytes_to_github(
        &bytes,
        &github_config.owner,
        &github_config.repo,
        "db.csv",
        &github_config.branch,
    )
    .await
}

pub async fn load_db(github_config: Option<&GithubConfig>) -> BTreeMap<u64, GuildRules> {
    if let Ok(db) = load_rules_from_file("db.csv".to_string()) {
        db
    } else if let Some(github_config) = github_config
        && let Ok(db) = load_rules_from_github(github_config).await
    {
        db
    } else {
        BTreeMap::new()
    }
}

mod tests {
    use crate::{config_handler, github_handler};

    #[allow(unused_imports)]
    use super::*;

    #[tokio::test]
    async fn test_save_db_to_file() {
        save_rules_to_file(&load_db(None).await, "db_test.csv".to_string());
    }

    #[tokio::test]
    async fn test_save_db_equals() {
        assert_eq!(
            load_rules_from_file("db.csv".to_string()).unwrap(),
            load_rules_from_github(&GithubConfig::new().unwrap())
                .await
                .unwrap()
        )
    }

    #[test]
    fn test_rule_named_activity() {
        let row = CsvRow {
            guild_id: "0".to_string(),
            guild_name: "guild_name".to_string(),
            role_id: "0".to_string(),
            role_name: "role1".to_string(),
            role_type: "named-activity".to_string(),
            activity_names: "Game1;Game2".to_string(),
            comments: "".to_string(),
        };
        let rule: Rule = row.into();
        assert_eq!(
            rule,
            Rule {
                guild_id: 0,
                guild_name: "guild_name".to_string(),
                role_id: 0,
                role_name: "role1".to_string(),
                role_type: RoleType::NamedActivity,
                activities: ["Game1", "Game2"].iter().map(|s| s.to_string()).collect(),
                comments: "".to_string(),
            }
        )
    }

    #[test]
    fn test_rule_else() {
        let row = CsvRow {
            guild_id: "0".to_string(),
            guild_name: "guild_name".to_string(),
            role_id: "0".to_string(),
            role_name: "role1".to_string(),
            role_type: "else".to_string(),
            activity_names: "Game1;Game2".to_string(),
            comments: "".to_string(),
        };
        let rule: Rule = row.into();
        assert_eq!(
            rule,
            Rule {
                guild_id: 0,
                guild_name: "guild_name".to_string(),
                role_id: 0,
                role_name: "role1".to_string(),
                role_type: RoleType::Else,
                activities: ["Game1", "Game2"].iter().map(|s| s.to_string()).collect(),
                comments: "".to_string(),
            }
        )
    }

    #[test]
    fn test_guild_rules_activity() {
        let named_rule = Rule {
            guild_id: 0,
            guild_name: "guild_name".to_string(),
            role_id: 0,
            role_name: "role1".to_string(),
            role_type: RoleType::NamedActivity,
            activities: ["Game1", "Game2"].iter().map(|s| s.to_string()).collect(),
            comments: "".to_string(),
        };
        let else_rule = Rule {
            guild_id: 0,
            guild_name: "guild_name".to_string(),
            role_id: 0,
            role_name: "role1".to_string(),
            role_type: RoleType::Else,
            activities: ["Game1", "Game2"].iter().map(|s| s.to_string()).collect(),
            comments: "".to_string(),
        };

        let mut guild_rules = GuildRules::new();
        guild_rules
            .activities_rules
            .insert(named_rule.role_id, named_rule);
        guild_rules.default_rule = Some(else_rule);

        let user_activities = ["AGame1"].iter().map(|s| s.to_string()).collect();

        assert_eq!(
            guild_rules.matching_rules(user_activities),
            guild_rules.activities_rules.values().cloned().collect()
        );

        let user_activities = ["asd"].iter().map(|s| s.to_string()).collect();

        assert_eq!(
            guild_rules.matching_rules(user_activities),
            guild_rules.default_rule.iter().cloned().collect()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_octocrab_upload_file() {
        config_handler::start();
        let config = config_handler::EnvConfig::new().unwrap();
        github_handler::start(&config.github_config.as_ref().unwrap().token)
            .await
            .unwrap();
        save_db_to_github(
            &load_db(config.github_config.as_ref()).await,
            &config.github_config.as_ref().unwrap(),
        )
        .await
        .unwrap();
    }
}
