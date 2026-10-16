//! Google Classroom ids. A class links to the course it was made from and a student to the
//! Classroom user they came from, so a later sync finds the same class or student again.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A Classroom id was the empty string. Google never issues one, so it means a broken payload or a
/// hand-edited blob.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmptyIdError;

impl fmt::Display for EmptyIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a Classroom id is empty")
    }
}

impl std::error::Error for EmptyIdError {}

/// Ids are compared exactly as Google sent them, so unlike a `Label` they are not trimmed.
/// Deserialization goes through `parse`, so a stored blob cannot hold an id the sync could
/// never have produced.
macro_rules! classroom_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn parse(raw: &str) -> Result<$name, EmptyIdError> {
                $name::try_from(raw.to_owned())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = EmptyIdError;

            fn try_from(raw: String) -> Result<Self, Self::Error> {
                if raw.is_empty() {
                    Err(EmptyIdError)
                } else {
                    Ok($name(raw))
                }
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> String {
                id.0
            }
        }
    };
}

classroom_id! {
    /// A Classroom course, which a class is linked to.
    ClassroomCourseId
}

classroom_id! {
    /// A Classroom user, which a student is linked to.
    ClassroomUserId
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_keeps_exactly_the_text_it_was_given() {
        let id = ClassroomCourseId::parse(" 12 34 ").unwrap();
        assert_eq!(id.as_str(), " 12 34 ");
    }

    #[test]
    fn an_empty_id_is_refused() {
        assert_eq!(ClassroomCourseId::parse(""), Err(EmptyIdError));
        assert_eq!(ClassroomUserId::parse(""), Err(EmptyIdError));
    }

    #[test]
    fn an_id_is_stored_as_a_plain_string_and_an_empty_one_is_unreadable() {
        let id = ClassroomUserId::parse("1089").unwrap();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"1089\"");
        assert_eq!(serde_json::from_str::<ClassroomUserId>(&json).unwrap(), id);
        assert!(serde_json::from_str::<ClassroomUserId>("\"\"").is_err());
    }
}
