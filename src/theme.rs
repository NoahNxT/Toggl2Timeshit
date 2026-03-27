use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    Terminal,
    Light,
    Dark,
    TokyoNight,
    Dracula,
    Catppuccin,
    Cyberpunk,
}

pub const THEME_PREFERENCES: [ThemePreference; 7] = [
    ThemePreference::Terminal,
    ThemePreference::Dark,
    ThemePreference::Light,
    ThemePreference::TokyoNight,
    ThemePreference::Dracula,
    ThemePreference::Catppuccin,
    ThemePreference::Cyberpunk,
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThemePalette {
    pub panel: String,
    pub border: String,
    pub text: String,
    pub muted: String,
    pub accent: String,
    pub highlight: String,
    pub success: String,
    pub error: String,
}

impl ThemePalette {
    pub fn normalized(&self) -> Result<Self, String> {
        Ok(Self {
            panel: normalize_hex(&self.panel)?,
            border: normalize_hex(&self.border)?,
            text: normalize_hex(&self.text)?,
            muted: normalize_hex(&self.muted)?,
            accent: normalize_hex(&self.accent)?,
            highlight: normalize_hex(&self.highlight)?,
            success: normalize_hex(&self.success)?,
            error: normalize_hex(&self.error)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomTheme {
    pub id: String,
    pub name: String,
    pub palette: ThemePalette,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ThemeSelection {
    Builtin { theme: ThemePreference },
    Custom { id: String },
}

impl Default for ThemeSelection {
    fn default() -> Self {
        Self::Builtin {
            theme: ThemePreference::Terminal,
        }
    }
}

impl ThemeSelection {
    pub fn builtin(theme: ThemePreference) -> Self {
        Self::Builtin { theme }
    }

    pub fn custom(id: impl Into<String>) -> Self {
        Self::Custom { id: id.into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuiltinTheme {
    pub theme: ThemePreference,
    pub label: String,
    pub palette: ThemePalette,
}

pub fn builtin_themes() -> Vec<BuiltinTheme> {
    THEME_PREFERENCES
        .iter()
        .copied()
        .map(|theme| BuiltinTheme {
            theme,
            label: theme_label(theme).to_string(),
            palette: builtin_theme_palette(theme),
        })
        .collect()
}

pub fn builtin_theme_palette(theme: ThemePreference) -> ThemePalette {
    match theme {
        ThemePreference::Terminal => ThemePalette {
            panel: "#101924".to_string(),
            border: "#5f6b7a".to_string(),
            text: "#f3f4f6".to_string(),
            muted: "#9aa4b2".to_string(),
            accent: "#4f8cff".to_string(),
            highlight: "#ffd166".to_string(),
            success: "#52d273".to_string(),
            error: "#ff6b6b".to_string(),
        },
        ThemePreference::Dark => ThemePalette {
            panel: "#121c34".to_string(),
            border: "#2c4870".to_string(),
            text: "#dce6ff".to_string(),
            muted: "#96aac8".to_string(),
            accent: "#5ab4ff".to_string(),
            highlight: "#ffd278".to_string(),
            success: "#78dc8c".to_string(),
            error: "#ff7878".to_string(),
        },
        ThemePreference::Light => ThemePalette {
            panel: "#ffffff".to_string(),
            border: "#d2dceb".to_string(),
            text: "#1a202c".to_string(),
            muted: "#5a6e8c".to_string(),
            accent: "#4682eb".to_string(),
            highlight: "#ffa550".to_string(),
            success: "#24965a".to_string(),
            error: "#dc3c50".to_string(),
        },
        ThemePreference::TokyoNight => ThemePalette {
            panel: "#1a1b26".to_string(),
            border: "#414868".to_string(),
            text: "#c0caf5".to_string(),
            muted: "#7d84ad".to_string(),
            accent: "#7dcfff".to_string(),
            highlight: "#ff9e64".to_string(),
            success: "#9ece6a".to_string(),
            error: "#f7768e".to_string(),
        },
        ThemePreference::Dracula => ThemePalette {
            panel: "#282a36".to_string(),
            border: "#6272a4".to_string(),
            text: "#f8f8f2".to_string(),
            muted: "#bd93f9".to_string(),
            accent: "#ff79c6".to_string(),
            highlight: "#8be9fd".to_string(),
            success: "#50fa7b".to_string(),
            error: "#ff5555".to_string(),
        },
        ThemePreference::Catppuccin => ThemePalette {
            panel: "#1e1e2e".to_string(),
            border: "#585b70".to_string(),
            text: "#cdd6f4".to_string(),
            muted: "#a6adc8".to_string(),
            accent: "#cba6f7".to_string(),
            highlight: "#f9e2af".to_string(),
            success: "#a6e3a1".to_string(),
            error: "#f38ba8".to_string(),
        },
        ThemePreference::Cyberpunk => ThemePalette {
            panel: "#150a26".to_string(),
            border: "#7637b8".to_string(),
            text: "#e8f6ff".to_string(),
            muted: "#84caff".to_string(),
            accent: "#00f5ff".to_string(),
            highlight: "#ff59c2".to_string(),
            success: "#6fffb1".to_string(),
            error: "#ff608d".to_string(),
        },
    }
}

pub fn theme_label(theme: ThemePreference) -> &'static str {
    match theme {
        ThemePreference::Terminal => "Terminal",
        ThemePreference::Dark => "Midnight",
        ThemePreference::Light => "Snow",
        ThemePreference::TokyoNight => "Tokyo Night",
        ThemePreference::Dracula => "Dracula",
        ThemePreference::Catppuccin => "Catppuccin",
        ThemePreference::Cyberpunk => "Cyberpunk",
    }
}

pub fn theme_selection_label(selection: &ThemeSelection, customs: &[CustomTheme]) -> String {
    match selection {
        ThemeSelection::Builtin { theme } => theme_label(*theme).to_string(),
        ThemeSelection::Custom { id } => find_custom_theme(customs, id)
            .map(|theme| theme.name.clone())
            .unwrap_or_else(|| "Custom theme".to_string()),
    }
}

pub fn sorted_custom_themes(customs: &[CustomTheme]) -> Vec<CustomTheme> {
    let mut sorted = customs.to_vec();
    sorted.sort_by(|left, right| {
        let left_key = left.name.to_ascii_lowercase();
        let right_key = right.name.to_ascii_lowercase();
        left_key
            .cmp(&right_key)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.id.cmp(&right.id))
    });
    sorted
}

pub fn find_custom_theme<'a>(customs: &'a [CustomTheme], id: &str) -> Option<&'a CustomTheme> {
    customs.iter().find(|theme| theme.id == id)
}

pub fn next_theme_selection(current: &ThemeSelection, customs: &[CustomTheme]) -> ThemeSelection {
    cycle_theme_selection(current, customs, false)
}

pub fn previous_theme_selection(
    current: &ThemeSelection,
    customs: &[CustomTheme],
) -> ThemeSelection {
    cycle_theme_selection(current, customs, true)
}

pub fn validate_theme_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Theme name is required.".to_string());
    }
    if trimmed.chars().count() > 48 {
        return Err("Theme name must be 48 characters or fewer.".to_string());
    }
    Ok(trimmed.to_string())
}

fn cycle_theme_selection(
    current: &ThemeSelection,
    customs: &[CustomTheme],
    backwards: bool,
) -> ThemeSelection {
    let mut values = THEME_PREFERENCES
        .iter()
        .copied()
        .map(ThemeSelection::builtin)
        .collect::<Vec<_>>();
    values.extend(
        sorted_custom_themes(customs)
            .into_iter()
            .map(|theme| ThemeSelection::custom(theme.id)),
    );

    if values.is_empty() {
        return ThemeSelection::default();
    }

    let index = values
        .iter()
        .position(|value| value == current)
        .unwrap_or(0);
    let next_index = if backwards {
        if index == 0 {
            values.len() - 1
        } else {
            index - 1
        }
    } else if index + 1 >= values.len() {
        0
    } else {
        index + 1
    };

    values[next_index].clone()
}

fn normalize_hex(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    let Some(rest) = trimmed.strip_prefix('#') else {
        return Err("Colors must use #RRGGBB format.".to_string());
    };
    if rest.len() != 6 || !rest.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err("Colors must use #RRGGBB format.".to_string());
    }
    Ok(format!("#{}", rest.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom_theme(id: &str, name: &str) -> CustomTheme {
        CustomTheme {
            id: id.to_string(),
            name: name.to_string(),
            palette: builtin_theme_palette(ThemePreference::Dark),
            created_at: "2026-03-27T11:00:00+01:00".to_string(),
            updated_at: "2026-03-27T11:00:00+01:00".to_string(),
        }
    }

    #[test]
    fn palette_normalizes_hex_values() {
        let palette = ThemePalette {
            panel: "#ABCDEF".to_string(),
            border: "#112233".to_string(),
            text: "#445566".to_string(),
            muted: "#778899".to_string(),
            accent: "#AABBCC".to_string(),
            highlight: "#CCDDEE".to_string(),
            success: "#00FF00".to_string(),
            error: "#FF0000".to_string(),
        };

        assert_eq!(palette.normalized().unwrap().panel, "#abcdef");
    }

    #[test]
    fn cycle_orders_custom_themes_alphabetically_after_builtins() {
        let customs = vec![custom_theme("b", "Zen"), custom_theme("a", "Aurora")];

        assert_eq!(
            next_theme_selection(
                &ThemeSelection::builtin(ThemePreference::Cyberpunk),
                &customs
            ),
            ThemeSelection::custom("a")
        );
        assert_eq!(
            next_theme_selection(&ThemeSelection::custom("a"), &customs),
            ThemeSelection::custom("b")
        );
    }

    #[test]
    fn selection_label_uses_custom_name() {
        let customs = vec![custom_theme("aurora", "Aurora")];

        assert_eq!(
            theme_selection_label(&ThemeSelection::custom("aurora"), &customs),
            "Aurora"
        );
    }

    #[test]
    fn validate_theme_name_rejects_empty_values() {
        assert!(validate_theme_name("   ").is_err());
    }
}
