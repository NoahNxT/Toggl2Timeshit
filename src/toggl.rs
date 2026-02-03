use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;

use crate::models::{Project, TimeEntry, Workspace};

#[derive(Debug, Clone)]
pub enum TogglError {
    Unauthorized,
    PaymentRequired,
    RateLimited,
    ServerError(String),
    Network(String),
}

#[derive(Clone)]
pub struct TogglClient {
    client: Client,
    token: String,
}

impl TogglClient {
    pub fn new(token: String) -> Self {
        let client = Client::builder()
            .user_agent("timeshit-tui")
            .build()
            .expect("Failed to build HTTP client");
        Self { client, token }
    }

    pub fn fetch_time_entries(&self, start: &str, end: &str) -> Result<Vec<TimeEntry>, TogglError> {
        let base = "https://api.track.toggl.com/api/v9/me/time_entries";
        let url = reqwest::Url::parse_with_params(
            base,
            &[("start_date", start), ("end_date", end)],
        )
        .map_err(|err| TogglError::Network(err.to_string()))?;
        self.fetch(url.to_string())
    }

    pub fn fetch_workspaces(&self) -> Result<Vec<Workspace>, TogglError> {
        let url = "https://api.track.toggl.com/api/v9/workspaces".to_string();
        self.fetch(url)
    }

    pub fn fetch_projects(&self, workspace_id: u64) -> Result<Vec<Project>, TogglError> {
        let url = format!(
            "https://api.track.toggl.com/api/v9/workspaces/{}/projects",
            workspace_id
        );
        self.fetch(url)
    }

    fn fetch<T: DeserializeOwned>(&self, url: String) -> Result<T, TogglError> {
        let credentials = STANDARD.encode(format!("{}:api_token", self.token));
        let response = self
            .client
            .get(url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Basic {}", credentials))
            .send()
            .map_err(|err| TogglError::Network(err.to_string()))?;

        if response.status() == 401 || response.status() == 403 {
            return Err(TogglError::Unauthorized);
        }

        if response.status() == 402 {
            return Err(TogglError::PaymentRequired);
        }

        if response.status() == 429 {
            return Err(TogglError::RateLimited);
        }

        if response.status().is_server_error() {
            return Err(TogglError::ServerError(format!(
                "Toggl API error: {}",
                response.status()
            )));
        }

        if !response.status().is_success() {
            return Err(TogglError::Network(format!(
                "Toggl API error: {}",
                response.status()
            )));
        }

        response
            .json::<T>()
            .map_err(|err| TogglError::Network(err.to_string()))
    }
}
