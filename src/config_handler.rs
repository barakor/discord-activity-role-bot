use anyhow::{Error, Result};

use dotenv::dotenv;
use std::env;

pub struct EnvConfig {
    pub discord_token: String,
}

pub fn get_config() -> Result<EnvConfig, Error> {
    dotenv().ok();
    let discord_token = env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN must be set");

    Ok(EnvConfig { discord_token })
}
