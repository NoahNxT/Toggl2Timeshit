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
