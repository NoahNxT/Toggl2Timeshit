use std::collections::HashMap;

use crate::models::{Project, TimeEntry};
use crate::rounding::RoundingConfig;
use crate::rounding::round_seconds;

#[derive(Debug, Clone)]
pub struct GroupedEntry {
    pub description: String,
    pub total_hours: f64,
}

#[derive(Debug, Clone)]
pub struct GroupedProject {
    pub project_name: String,
    pub client_name: Option<String>,
    pub display_name: String,
    pub total_hours: f64,
    pub entries: Vec<GroupedEntry>,
}

pub fn group_entries(
    entries: &[TimeEntry],
    projects: &[Project],
    client_names: &HashMap<u64, String>,
    rounding: Option<&RoundingConfig>,
) -> Vec<GroupedProject> {
    let mut project_info: HashMap<Option<u64>, (String, Option<String>)> = HashMap::new();
    for project in projects {
        let client_name = project.client_name.clone().or_else(|| {
            project
                .client_id
                .and_then(|id| client_names.get(&id).cloned())
        });
        project_info.insert(Some(project.id), (project.name.clone(), client_name));
    }
    project_info.insert(None, ("No Project".to_string(), None));

    let mut grouped: HashMap<Option<u64>, HashMap<String, i64>> = HashMap::new();

    for entry in entries {
        let project_key = entry.project_id;
        let description = entry
            .description
            .clone()
            .unwrap_or_else(|| "No description".to_string());
        let project_entries = grouped.entry(project_key).or_default();
        *project_entries.entry(description).or_insert(0) += entry.duration;
    }

    let mut result: Vec<GroupedProject> = grouped
        .into_iter()
        .map(|(project_id, entries)| {
            let (project_name, client_name) = project_info
                .get(&project_id)
                .cloned()
                .unwrap_or_else(|| ("Unknown Project".to_string(), None));
            let display_name = match &client_name {
                Some(client) => format!("{client} — {project_name}"),
                None => project_name.clone(),
            };

            let mut total_seconds = 0i64;
            let mut entry_list: Vec<GroupedEntry> = entries
                .into_iter()
                .map(|(description, duration)| {
                    let rounded_seconds = rounding
                        .map(|cfg| round_seconds(duration, cfg))
                        .unwrap_or(duration);
                    total_seconds = total_seconds.saturating_add(rounded_seconds);
                    GroupedEntry {
                        description,
                        total_hours: rounded_seconds as f64 / 3600.0,
                    }
                })
                .collect();

            entry_list.sort_by(|a, b| b.total_hours.partial_cmp(&a.total_hours).unwrap());

            GroupedProject {
                project_name,
                client_name,
                display_name,
                total_hours: total_seconds as f64 / 3600.0,
                entries: entry_list,
            }
        })
        .collect();

    result.sort_by(|a, b| {
        match (&a.client_name, &b.client_name) {
            (Some(a_client), Some(b_client)) => a_client.cmp(b_client),
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, None) => std::cmp::Ordering::Equal,
        }
        .then_with(|| b.total_hours.partial_cmp(&a.total_hours).unwrap())
        .then_with(|| a.project_name.cmp(&b.project_name))
    });

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rounding::{RoundingConfig, RoundingMode};
    use std::collections::HashMap;

    #[test]
    fn groups_entries_by_project_and_description() {
        let projects = vec![
            Project {
                id: 1,
                name: "Project A".to_string(),
                client_id: None,
                client_name: None,
            },
            Project {
                id: 2,
                name: "Project B".to_string(),
                client_id: None,
                client_name: None,
            },
        ];

        let entries = vec![
            TimeEntry {
                id: 1,
                description: Some("Ticket 1".to_string()),
                duration: 3600,
                start: "2026-02-03T00:00:00Z".to_string(),
                stop: Some("2026-02-03T01:00:00Z".to_string()),
                project_id: Some(1),
            },
            TimeEntry {
                id: 2,
                description: Some("Ticket 1".to_string()),
                duration: 1800,
                start: "2026-02-03T02:00:00Z".to_string(),
                stop: Some("2026-02-03T02:30:00Z".to_string()),
                project_id: Some(1),
            },
            TimeEntry {
                id: 3,
                description: Some("Ticket 2".to_string()),
                duration: 1800,
                start: "2026-02-03T03:00:00Z".to_string(),
                stop: Some("2026-02-03T03:30:00Z".to_string()),
                project_id: Some(2),
            },
        ];

        let grouped = group_entries(&entries, &projects, &HashMap::new(), None);
        assert_eq!(grouped.len(), 2);
        let project_a = grouped
            .iter()
            .find(|g| g.project_name == "Project A")
            .unwrap();
        assert_eq!(project_a.entries.len(), 1);
        assert!((project_a.total_hours - 1.5).abs() < 0.01);
    }

    #[test]
    fn rounds_each_grouped_entry_and_totals_are_sum_of_rounded_lines() {
        let projects = vec![Project {
            id: 1,
            name: "Project A".to_string(),
            client_id: None,
            client_name: None,
        }];

        let entries = vec![
            TimeEntry {
                id: 1,
                description: Some("Ticket 1".to_string()),
                duration: 14 * 60,
                start: "2026-02-03T00:00:00Z".to_string(),
                stop: Some("2026-02-03T00:14:00Z".to_string()),
                project_id: Some(1),
            },
            TimeEntry {
                id: 2,
                description: Some("Ticket 2".to_string()),
                duration: 14 * 60,
                start: "2026-02-03T01:00:00Z".to_string(),
                stop: Some("2026-02-03T01:14:00Z".to_string()),
                project_id: Some(1),
            },
        ];

        let rounding = RoundingConfig {
            increment_minutes: 15,
            mode: RoundingMode::Closest,
        };

        let grouped = group_entries(&entries, &projects, &HashMap::new(), Some(&rounding));
        assert_eq!(grouped.len(), 1);
        let project_a = &grouped[0];

        let ticket1 = project_a
            .entries
            .iter()
            .find(|e| e.description == "Ticket 1")
            .unwrap();
        let ticket2 = project_a
            .entries
            .iter()
            .find(|e| e.description == "Ticket 2")
            .unwrap();
        assert!((ticket1.total_hours - 0.25).abs() < 0.001);
        assert!((ticket2.total_hours - 0.25).abs() < 0.001);
        assert!((project_a.total_hours - 0.5).abs() < 0.001);
    }
}
