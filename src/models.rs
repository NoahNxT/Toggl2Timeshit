use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Workspace {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Project {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TimeEntry {
    pub id: u64,
    pub description: Option<String>,
    pub duration: i64,
    pub start: String,
    pub stop: Option<String>,
    #[serde(rename = "project_id")]
    pub project_id: Option<u64>,
}
