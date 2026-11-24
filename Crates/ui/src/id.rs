//! Stable UI identifiers.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Stable identifier for a UI node.
///
/// IDs come from documents or code and survive reloads. The ECS `Entity` is
/// not stable across a document reload, so interaction and focus state are
/// restored by [`UiId`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UiId(String);

impl UiId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for UiId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for UiId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<String> for UiId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl AsRef<str> for UiId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
