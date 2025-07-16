use anyhow::{Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use base64::{Engine as _, engine::general_purpose};
use bytes::Bytes;
use octocrab::Octocrab;
use reqwest::Client;
use std::io::Cursor;

use crate::config_handler::get_config;

pub async fn get_bytes_buffer_from_url(url: &str) -> Result<Cursor<Bytes>> {
    let response = Client::new().get(url).send().await?.bytes().await?;
    Ok(Cursor::new(response))
}

pub async fn get_bytes_from_github(
    owner: &str,
    repo: &str,
    path_in_repo: &str,
    branch: &str,
) -> Result<Vec<u8>> {
    let octocrab = octocrab::instance();
    let file_response = octocrab
        .repos(owner, repo)
        .get_content()
        .path(path_in_repo)
        .r#ref(branch)
        .send()
        .await?;

    // file_response.items is a Vec<ContentItem>

    match file_response.items.into_iter().next() {
        Some(base64_file_content) => Ok(STANDARD.decode(
            base64_file_content
                .content
                .ok_or(anyhow!("missing content"))?
                .replace('\n', ""),
        )?),
        None => Err(anyhow!("Couldn't get file content")),
    }
}

pub async fn upload_bytes_to_github(
    data: &Bytes,
    owner: &str,
    repo: &str,
    path_in_repo: &str,
    branch: &str,
) -> Result<()> {
    let octocrab = octocrab::instance();
    let encoded = general_purpose::STANDARD.encode(data);

    // Try to get the existing file to obtain its SHA
    let file = octocrab
        .repos(owner, repo)
        .get_content()
        .path(path_in_repo)
        // .r#ref(branch)
        .send()
        .await
        .ok();

    let sha = file
        .as_ref()
        .and_then(|content| content.items.first())
        .map(|item| item.sha.clone())
        .ok_or(anyhow!("Failed to get file SHA"))?;

    // Now update or create the file
    octocrab
        .repos(owner, repo)
        .update_file(path_in_repo, "Update rules DB", encoded, sha)
        .branch(branch)
        .send()
        .await?;

    Ok(())
}

pub async fn start(github_token: &Option<String>) -> Result<()> {
    let octocrab_client = match github_token {
        Some(pat) => Octocrab::builder()
            .personal_token(pat.to_string())
            .build()?,
        None => Octocrab::builder().build()?,
    };
    octocrab::initialise(octocrab_client);
    Ok({})
}

#[cfg(test)]
mod tests {

    use crate::rules_handler::{load_rules_from_buffer, load_rules_from_github};

    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_octocrab() {
        let data =
            get_bytes_from_github("barakor", "discord-activity-role-bot", "db.csv", "db-data")
                .await
                .unwrap();
        let rules = load_rules_from_buffer(data.as_slice()).unwrap();

        assert_eq!(rules, load_rules_from_github().await.unwrap());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_octocrab_upload_file() {
        upload_bytes_to_github(
            &Bytes::from("Hello"),
            "barakor",
            "discord-activity-role-bot",
            "db.csv",
            "db-data",
        )
        .await
        .unwrap();
    }
}
