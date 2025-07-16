use anyhow::{Error, Result};

use dotenv::dotenv;
use std::env;

pub struct EnvConfig {
    pub discord_token: String,
    pub github_token: Option<String>,
}

pub fn get_config() -> Result<EnvConfig, Error> {
    dotenv().ok();

    Ok(EnvConfig {
        discord_token: env::var("DISCORD_TOKEN")?,
        github_token: env::var("GITHUB_TOKEN").ok(),
    })
}

#[allow(dead_code)]
pub fn get_testing_config() -> Result<EnvConfig, Error> {
    dotenv().ok();

    Ok(EnvConfig {
        discord_token: env::var("DISCORD_TESTING_TOKEN")?,
        github_token: Some(env::var("GITHUB_TOKEN")?),
    })
}
