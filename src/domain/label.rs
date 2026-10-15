use std::fmt;

use serde::{Deserialize, Serialize};

pub const MAX_LABEL_CHARS: usize = 40;

/// A trimmed, non-empty, bounded display string shared by class, student, and group names.
///
/// Deserialization goes through [`Label::parse`], so a stored blob cannot smuggle in a name the
/// UI could never have produced.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Label(String);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelError {
    Empty,
    TooLong,
}

impl fmt::Display for LabelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LabelError::Empty => f.write_str("name is empty"),
            LabelError::TooLong => write!(f, "name is longer than {MAX_LABEL_CHARS} characters"),
        }
    }
}

impl Label {
    pub fn parse(raw: &str) -> Result<Label, LabelError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(LabelError::Empty);
        }
        if trimmed.chars().count() > MAX_LABEL_CHARS {
            return Err(LabelError::TooLong);
        }
        Ok(Label(trimmed.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Label {
    type Error = LabelError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Label::parse(&raw)
    }
}

impl From<Label> for String {
    fn from(label: Label) -> String {
        label.0
    }
}

impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_trims_surrounding_whitespace() {
        assert_eq!(Label::parse("  Ava \t").unwrap().as_str(), "Ava");
    }

    #[test]
    fn parse_rejects_blank_input() {
        assert_eq!(Label::parse(""), Err(LabelError::Empty));
        assert_eq!(Label::parse(" \t\n "), Err(LabelError::Empty));
    }

    #[test]
    fn parse_bounds_length_in_characters_not_bytes() {
        let at_limit = "é".repeat(MAX_LABEL_CHARS);
        assert!(Label::parse(&at_limit).is_ok());
        let over = "é".repeat(MAX_LABEL_CHARS + 1);
        assert_eq!(Label::parse(&over), Err(LabelError::TooLong));
    }

    #[test]
    fn length_limit_ignores_trimmed_whitespace() {
        let padded = format!("  {}  ", "a".repeat(MAX_LABEL_CHARS));
        assert!(Label::parse(&padded).is_ok());
    }

    #[test]
    fn serde_round_trips_as_a_plain_string() {
        let label = Label::parse("Brave Bears").unwrap();
        let json = serde_json::to_string(&label).unwrap();
        assert_eq!(json, "\"Brave Bears\"");
        assert_eq!(serde_json::from_str::<Label>(&json).unwrap(), label);
    }

    #[test]
    fn deserialize_rejects_invalid_names() {
        assert!(serde_json::from_str::<Label>("\"   \"").is_err());
        let too_long = format!("\"{}\"", "x".repeat(MAX_LABEL_CHARS + 1));
        assert!(serde_json::from_str::<Label>(&too_long).is_err());
    }

    #[test]
    fn deserialize_trims_like_parse() {
        let label: Label = serde_json::from_str("\"  Ben \"").unwrap();
        assert_eq!(label.as_str(), "Ben");
    }
}
