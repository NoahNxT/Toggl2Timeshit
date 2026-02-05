use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoundingMode {
    Closest,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoundingConfig {
    pub increment_minutes: u32,
    pub mode: RoundingMode,
}

impl Default for RoundingConfig {
    fn default() -> Self {
        Self {
            increment_minutes: 15,
            mode: RoundingMode::Closest,
        }
    }
}

pub fn round_seconds(seconds: i64, cfg: &RoundingConfig) -> i64 {
    if cfg.increment_minutes == 0 {
        return seconds;
    }

    let increment_seconds = i64::from(cfg.increment_minutes) * 60;
    if increment_seconds <= 0 {
        return seconds;
    }

    let sign = if seconds < 0 { -1 } else { 1 };
    let abs_seconds = seconds.abs();

    let rounded = match cfg.mode {
        RoundingMode::Down => (abs_seconds / increment_seconds) * increment_seconds,
        RoundingMode::Up => {
            if abs_seconds % increment_seconds == 0 {
                abs_seconds
            } else {
                ((abs_seconds / increment_seconds) + 1) * increment_seconds
            }
        }
        RoundingMode::Closest => {
            let lower = (abs_seconds / increment_seconds) * increment_seconds;
            let upper = if abs_seconds % increment_seconds == 0 {
                lower
            } else {
                lower + increment_seconds
            };
            let distance_to_lower = abs_seconds - lower;
            let distance_to_upper = upper - abs_seconds;
            if distance_to_upper <= distance_to_lower {
                upper
            } else {
                lower
            }
        }
    };

    rounded.saturating_mul(sign)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_seconds_off_returns_raw() {
        let cfg = RoundingConfig {
            increment_minutes: 0,
            mode: RoundingMode::Closest,
        };
        assert_eq!(round_seconds(123, &cfg), 123);
        assert_eq!(round_seconds(-123, &cfg), -123);
    }

    #[test]
    fn round_seconds_up() {
        let cfg = RoundingConfig {
            increment_minutes: 15,
            mode: RoundingMode::Up,
        };
        assert_eq!(round_seconds(1, &cfg), 900);
        assert_eq!(round_seconds(900, &cfg), 900);
        assert_eq!(round_seconds(901, &cfg), 1800);
    }

    #[test]
    fn round_seconds_down() {
        let cfg = RoundingConfig {
            increment_minutes: 15,
            mode: RoundingMode::Down,
        };
        assert_eq!(round_seconds(899, &cfg), 0);
        assert_eq!(round_seconds(900, &cfg), 900);
        assert_eq!(round_seconds(1799, &cfg), 900);
    }

    #[test]
    fn round_seconds_closest_ties_up() {
        let cfg = RoundingConfig {
            increment_minutes: 15,
            mode: RoundingMode::Closest,
        };
        assert_eq!(round_seconds(449, &cfg), 0);
        assert_eq!(round_seconds(450, &cfg), 900);
        assert_eq!(round_seconds(451, &cfg), 900);
        assert_eq!(round_seconds(1349, &cfg), 900);
        assert_eq!(round_seconds(1350, &cfg), 1800);
    }

    #[test]
    fn round_seconds_negative_values_keep_sign() {
        let cfg = RoundingConfig {
            increment_minutes: 15,
            mode: RoundingMode::Closest,
        };
        assert_eq!(round_seconds(-450, &cfg), -900);
        assert_eq!(round_seconds(-449, &cfg), 0);
    }
}
