use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::BufReader,
};

use serde::Deserialize;

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

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GuildRules {
    pub default_rules: BTreeSet<Rule>,
    pub activities_rules: BTreeSet<Rule>,
}

impl GuildRules {
    pub fn new() -> Self {
        GuildRules {
            default_rules: BTreeSet::new(),
            activities_rules: BTreeSet::new(),
        }
    }

    pub fn all_rules(&self) -> BTreeSet<&Rule> {
        self.activities_rules.union(&self.default_rules).collect()
    }

    pub fn matching_rules(&self, user_activities: BTreeSet<String>) -> BTreeSet<&Rule> {
        let activity_rules: BTreeSet<&Rule> = self
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
            .collect();
        match activity_rules.is_empty() {
            false => activity_rules,
            true if !user_activities.is_empty() => self.default_rules.iter().collect(),
            true => BTreeSet::new(),
        }
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
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
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

#[derive(Debug, Deserialize)]
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

pub fn load_rules() -> BTreeMap<u64, GuildRules> {
    let file = File::open("db.csv").expect("CSV file not found");
    let mut rdr = csv::Reader::from_reader(BufReader::new(file));
    let mut rules = BTreeMap::new();

    for result in rdr.deserialize() {
        let row: CsvRow = result.expect("Error reading row");
        let rule: Rule = row.into();

        let guild_rules = rules.entry(rule.guild_id).or_insert(GuildRules::new());

        match rule.role_type {
            RoleType::NamedActivity => {
                guild_rules.activities_rules.insert(rule);
            }
            RoleType::Else => {
                guild_rules.default_rules.insert(rule);
            }
        }

        // rules.insert(guild_id, rule);
    }

    rules
}

mod tests {
    #[allow(unused_imports)]
    use super::*;
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
        guild_rules.default_rules.insert(else_rule);

        let user_activities = ["AGame1"].iter().map(|s| s.to_string()).collect();

        assert_eq!(
            guild_rules.matching_rules(user_activities),
            guild_rules.activities_rules.iter().collect()
        );

        let user_activities = ["asd"].iter().map(|s| s.to_string()).collect();

        assert_eq!(
            guild_rules.matching_rules(user_activities),
            guild_rules.default_rules.iter().collect()
        );
    }
}
