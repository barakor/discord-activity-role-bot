use anyhow::{Error, Result};

use dotenv::dotenv;
use std::{
    env,
    sync::atomic::{AtomicBool, Ordering},
};

static CONFIG_HANDLER_STARTED: AtomicBool = AtomicBool::new(false);

/// Mark the config handler as started or not.
pub fn set_config_handler_started(started: bool) {
    CONFIG_HANDLER_STARTED.store(started, Ordering::SeqCst);
}

pub fn is_config_handler_started() -> bool {
    CONFIG_HANDLER_STARTED.load(Ordering::SeqCst)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GithubConfig {
    pub token: Option<String>,
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub path: String,
}

impl GithubConfig {
    pub fn new() -> Result<Self, Error> {
        start()?;
        Ok(Self {
            token: env::var("GITHUB_TOKEN").ok(),
            owner: env::var("GITHUB_OWNER")?,
            repo: env::var("GITHUB_REPO")?,
            branch: env::var("GITHUB_BRANCH")?,
            path: env::var("GITHUB_PATH")?,
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EnvConfig {
    pub discord_token: String,
    pub github_config: Option<GithubConfig>,
}

impl EnvConfig {
    pub fn new() -> Result<Self, Error> {
        start()?;
        Ok(Self {
            discord_token: env::var("DISCORD_TOKEN")?,
            github_config: GithubConfig::new().ok(),
        })
    }
}

#[allow(dead_code)]
pub fn get_testing_config() -> Result<EnvConfig, Error> {
    start()?;

    Ok(EnvConfig {
        discord_token: env::var("DISCORD_TESTING_TOKEN")?,
        github_config: GithubConfig::new().ok(),
    })
}

pub fn start() -> Result<(), Error> {
    if !is_config_handler_started() {
        dotenv().ok();
        set_config_handler_started(true);
    }
    Ok(())
}
