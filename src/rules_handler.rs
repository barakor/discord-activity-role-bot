use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::BufReader,
};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use twilight_model::channel::message::embed::EmbedField;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RoleType {
    NamedActivity,
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
    activities_rules: BTreeSet<Rule>,
    default_rule: Option<Rule>,
}

impl GuildRules {
    pub fn new() -> Self {
        GuildRules {
            default_rule: None,
            activities_rules: BTreeSet::new(),
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
            .union(&self.activities_rules)
            .cloned()
            .collect()
    }

    pub fn matching_rules(&self, user_activities: BTreeSet<String>) -> BTreeSet<Rule> {
        if user_activities.is_empty() {
            return BTreeSet::new();
        };

        let activity_rules: BTreeSet<Rule> = self
            .activities_rules
            .iter()
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
}

impl Into<Vec<EmbedField>> for GuildRules {
    fn into(self) -> Vec<EmbedField> {
        self.activities_rules
            .iter()
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
        rows.extend(self.activities_rules.iter().map(|r| r.clone().into()));
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

pub fn load_rules_from_file(file_path: String) -> Result<BTreeMap<u64, GuildRules>> {
    let file = File::open(file_path)?;
    let mut rdr = csv::Reader::from_reader(BufReader::new(file));
    let mut rules = BTreeMap::new();

    for result in rdr.deserialize() {
        let row: CsvRow = result?;
        let rule: Rule = row.into();

        let guild_rules = rules.entry(rule.guild_id).or_insert(GuildRules::new());

        match rule.role_type {
            RoleType::NamedActivity => {
                guild_rules.activities_rules.insert(rule);
            }
            RoleType::Else => {
                guild_rules.default_rule = Some(rule);
            }
        }
    }

    Ok(rules)
}

pub fn save_rules_to_file(rules: &BTreeMap<u64, GuildRules>, file_path: String) -> Result<()> {
    let file = File::create(file_path)?;
    let mut wtr = csv::Writer::from_writer(file);

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
    Ok(())
}

pub fn save_db_to_file(rules: &BTreeMap<u64, GuildRules>) -> Result<()> {
    save_rules_to_file(rules, "db.csv".to_string())
}

pub fn load_db_from_file() -> BTreeMap<u64, GuildRules> {
    match load_rules_from_file("db.csv".to_string()) {
        Ok(rules) => rules,
        Err(_) => panic!(), // should load from github
    }
}

mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_save_db_to_file() {
        save_rules_to_file(&load_db_from_file(), "db_test.csv".to_string());
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
        guild_rules.activities_rules.insert(named_rule);
        guild_rules.default_rule = Some(else_rule);

        let user_activities = ["AGame1"].iter().map(|s| s.to_string()).collect();

        assert_eq!(
            guild_rules.matching_rules(user_activities),
            guild_rules.activities_rules.iter().cloned().collect()
        );

        let user_activities = ["asd"].iter().map(|s| s.to_string()).collect();

        assert_eq!(
            guild_rules.matching_rules(user_activities),
            guild_rules.default_rule.iter().cloned().collect()
        );
    }
}
