use std::{borrow::Borrow, fmt, ops::Deref, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use serde_json::Value;
use thiserror::Error;

/// A single validated Keenetic CLI command.
///
/// The command remains opaque because the available grammar depends on the
/// router model, installed components, and `KeeneticOS` version.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CliCommand(Box<str>);

impl CliCommand {
    /// Parses and validates one CLI command.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidCliCommand`] for a blank command or a command containing
    /// control characters.
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidCliCommand> {
        Self::try_from(value.into())
    }

    /// Returns the command exactly as it will be sent to the router.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<Box<str>> for CliCommand {
    type Error = InvalidCliCommand;

    fn try_from(value: Box<str>) -> Result<Self, Self::Error> {
        if value.trim().is_empty() {
            return Err(InvalidCliCommand::Empty);
        }
        if value.chars().any(char::is_control) {
            return Err(InvalidCliCommand::ControlCharacter);
        }
        Ok(Self(value))
    }
}

impl TryFrom<String> for CliCommand {
    type Error = InvalidCliCommand;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_from(value.into_boxed_str())
    }
}

impl TryFrom<&str> for CliCommand {
    type Error = InvalidCliCommand;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl FromStr for CliCommand {
    type Err = InvalidCliCommand;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(Box::<str>::from(value))
    }
}

impl AsRef<str> for CliCommand {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for CliCommand {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl Deref for CliCommand {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl fmt::Debug for CliCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("CliCommand")
            .field(&"[REDACTED]")
            .finish()
    }
}

impl fmt::Display for CliCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for CliCommand {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CliCommand {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        <Box<str>>::deserialize(deserializer)?
            .try_into()
            .map_err(de::Error::custom)
    }
}

/// A CLI command is not a valid single request.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum InvalidCliCommand {
    /// The command was empty or contained only whitespace.
    #[error("a CLI command must contain a non-whitespace character")]
    Empty,
    /// The command contained a newline or another control character.
    #[error("a CLI command must not contain control characters")]
    ControlCharacter,
}

/// A lossless reply returned by the Keenetic CLI parser.
///
/// CLI response fields depend on the command and installed router components.
/// The raw JSON is therefore retained while common console fields have typed
/// accessors.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct CliReply(Value);

impl CliReply {
    /// Iterates over text lines emitted through the router console.
    #[must_use = "iterators are lazy and do nothing unless consumed"]
    pub fn tty_output(&self) -> impl Iterator<Item = &str> {
        self.0
            .get("tty-out")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
    }

    /// Returns the prompt reported after command execution, when present.
    #[must_use]
    pub fn prompt(&self) -> Option<&str> {
        self.0.get("prompt").and_then(Value::as_str)
    }

    /// Returns the complete response without discarding command-specific data.
    #[must_use]
    pub const fn raw(&self) -> &Value {
        &self.0
    }

    /// Consumes the reply and returns the complete response.
    #[must_use]
    pub fn into_raw(self) -> Value {
        self.0
    }
}

impl AsRef<Value> for CliReply {
    fn as_ref(&self) -> &Value {
        self.raw()
    }
}

impl From<CliReply> for Value {
    fn from(reply: CliReply) -> Self {
        reply.into_raw()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{CliCommand, CliReply, InvalidCliCommand};

    #[test]
    fn command_rejects_empty_and_multiline_input() {
        assert_eq!("".parse::<CliCommand>(), Err(InvalidCliCommand::Empty));
        assert_eq!("   ".parse::<CliCommand>(), Err(InvalidCliCommand::Empty));
        assert_eq!(
            "show version\nsystem reboot".parse::<CliCommand>(),
            Err(InvalidCliCommand::ControlCharacter)
        );
    }

    #[test]
    fn command_serializes_as_a_json_string() {
        let command = CliCommand::new("interface UsbLte1 tty send AT+GTCAINFO?").unwrap();
        assert_eq!(
            serde_json::to_value(&command).unwrap(),
            json!("interface UsbLte1 tty send AT+GTCAINFO?")
        );
        assert_eq!(
            serde_json::from_value::<CliCommand>(json!(command.as_str())).unwrap(),
            command
        );
    }

    #[test]
    fn command_debug_does_not_expose_its_contents() {
        let command = CliCommand::new("user admin password unique-sensitive-value").unwrap();
        let debug = format!("{command:?}");

        assert_eq!(debug, "CliCommand(\"[REDACTED]\")");
        assert!(!debug.contains("unique-sensitive-value"));
    }

    #[test]
    fn reply_preserves_unknown_fields_and_exposes_console_output() {
        let value = json!({
            "tty-out": ["PCC: 103,489,1275", "OK", 42],
            "prompt": "(config)",
            "future-field": {"nested": true}
        });
        let reply: CliReply = serde_json::from_value(value.clone()).unwrap();

        assert_eq!(
            reply.tty_output().collect::<Vec<_>>(),
            ["PCC: 103,489,1275", "OK"]
        );
        assert_eq!(reply.prompt(), Some("(config)"));
        assert_eq!(reply.raw(), &value);
        assert_eq!(reply.into_raw(), value);
    }
}
