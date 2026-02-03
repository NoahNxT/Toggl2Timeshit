use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Workspace {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Project {
    pub id: u64,
    pub name: String,
    #[serde(rename = "client_id")]
    pub client_id: Option<u64>,
    #[serde(rename = "client_name")]
    pub client_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Client {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TimeEntry {
    pub id: u64,
    pub description: Option<String>,
    pub duration: i64,
    pub start: String,
    pub stop: Option<String>,
    #[serde(rename = "project_id")]
    pub project_id: Option<u64>,
}
