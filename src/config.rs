//! Startup configuration, read from the environment and validated eagerly.

use crate::rules::Rules;
use crate::table::MAX_SEATS;
use std::env;
use std::error::Error;
use std::fmt;
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub bind_addr: String,
    pub rules: Rules,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigError {
    Missing(&'static str),
    Unparsable { var: &'static str, value: String },
    OutOfRange { var: &'static str, expected: String },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(var) => write!(f, "{} is not set (see .env.example)", var),
            Self::Unparsable { var, value } => {
                write!(f, "{} is not a valid value: {:?}", var, value)
            }
            Self::OutOfRange { var, expected } => write!(f, "{} must be {}", var, expected),
        }
    }
}

impl Error for ConfigError {}

impl Config {
    /// Reads the process environment. Fails on the first problem, naming the
    /// variable, rather than starting a table on silent defaults.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::read(|var| env::var(var).ok())
    }

    /// Environment-shaped but source-agnostic, so the rules below are testable
    /// without mutating global process state.
    pub fn read(get: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let config = Config {
            bind_addr: required("BLACKJACK_BIND_ADDR", &get)?,
            rules: Rules {
                packs: parsed("BLACKJACK_PACKS", 6, &get)?,
                dealer_hits_soft_17: parsed("BLACKJACK_DEALER_HITS_SOFT_17", false, &get)?,
                min_bet: parsed("BLACKJACK_MIN_BET", 5, &get)?,
                starting_bankroll: parsed("BLACKJACK_STARTING_BANKROLL", 200, &get)?,
                min_players: parsed("BLACKJACK_MIN_PLAYERS", 1, &get)?,
            },
        };
        config.validated()
    }

    fn validated(self) -> Result<Self, ConfigError> {
        let Rules { packs, min_bet, starting_bankroll, min_players, .. } = self.rules;
        range("BLACKJACK_PACKS", "between 1 and 8", (1..=8).contains(&packs))?;
        range("BLACKJACK_MIN_BET", "at least 1", min_bet >= 1)?;
        let affordable = starting_bankroll >= min_bet;
        range("BLACKJACK_STARTING_BANKROLL", "at least BLACKJACK_MIN_BET", affordable)?;
        let seats = (1..=MAX_SEATS).contains(&min_players);
        range("BLACKJACK_MIN_PLAYERS", "between 1 and 7", seats)?;
        Ok(self)
    }
}

fn required(
    var: &'static str,
    get: &impl Fn(&str) -> Option<String>,
) -> Result<String, ConfigError> {
    get(var).filter(|value| !value.trim().is_empty()).ok_or(ConfigError::Missing(var))
}

fn parsed<T: FromStr>(
    var: &'static str,
    default: T,
    get: &impl Fn(&str) -> Option<String>,
) -> Result<T, ConfigError> {
    let Some(raw) = get(var).filter(|value| !value.trim().is_empty()) else {
        return Ok(default);
    };
    raw.trim().parse().map_err(|_| ConfigError::Unparsable { var, value: raw })
}

fn range(var: &'static str, expected: &str, ok: bool) -> Result<(), ConfigError> {
    if ok {
        return Ok(());
    }
    Err(ConfigError::OutOfRange { var, expected: expected.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn source(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> =
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |var| map.get(var).cloned()
    }

    fn valid() -> Vec<(&'static str, &'static str)> {
        vec![("BLACKJACK_BIND_ADDR", "127.0.0.1:8080")]
    }

    fn with(extra: (&'static str, &'static str)) -> Result<Config, ConfigError> {
        let mut pairs = valid();
        pairs.push(extra);
        Config::read(source(&pairs))
    }

    #[test]
    fn a_missing_bind_address_is_named_in_the_error() {
        let error = Config::read(source(&[])).unwrap_err();
        assert_eq!(error, ConfigError::Missing("BLACKJACK_BIND_ADDR"));
        assert!(error.to_string().contains("BLACKJACK_BIND_ADDR"));
    }

    #[test]
    fn a_blank_variable_counts_as_missing() {
        let error = Config::read(source(&[("BLACKJACK_BIND_ADDR", "   ")])).unwrap_err();
        assert_eq!(error, ConfigError::Missing("BLACKJACK_BIND_ADDR"));
    }

    #[test]
    fn documented_defaults_apply_when_only_the_address_is_set() {
        let config = Config::read(source(&valid())).expect("defaults are valid");
        assert_eq!(config.bind_addr, "127.0.0.1:8080");
        assert_eq!(config.rules.packs, 6);
        assert_eq!(config.rules.min_bet, 5);
        assert!(!config.rules.dealer_hits_soft_17);
    }

    #[test]
    fn a_non_numeric_override_fails_loudly_instead_of_falling_back() {
        let error = with(("BLACKJACK_MIN_BET", "five")).unwrap_err();
        let expected = ConfigError::Unparsable { var: "BLACKJACK_MIN_BET", value: "five".into() };
        assert_eq!(error, expected);
    }

    #[test]
    fn the_soft_17_switch_reads_as_a_boolean() {
        assert!(
            with(("BLACKJACK_DEALER_HITS_SOFT_17", "true"))
                .expect("valid")
                .rules
                .dealer_hits_soft_17
        );
        assert!(with(("BLACKJACK_DEALER_HITS_SOFT_17", "yes")).is_err());
    }

    #[test]
    fn a_shoe_outside_one_to_eight_packs_is_rejected() {
        assert!(with(("BLACKJACK_PACKS", "0")).is_err());
        assert!(with(("BLACKJACK_PACKS", "9")).is_err());
        assert!(with(("BLACKJACK_PACKS", "8")).is_ok());
    }

    #[test]
    fn a_bankroll_that_cannot_cover_the_minimum_bet_is_rejected() {
        let error = with(("BLACKJACK_STARTING_BANKROLL", "1")).unwrap_err();
        assert!(error.to_string().contains("BLACKJACK_MIN_BET"));
    }

    #[test]
    fn min_players_cannot_exceed_the_seat_count() {
        assert!(with(("BLACKJACK_MIN_PLAYERS", "8")).is_err());
        assert!(with(("BLACKJACK_MIN_PLAYERS", "0")).is_err());
        assert!(with(("BLACKJACK_MIN_PLAYERS", "7")).is_ok());
    }
}
