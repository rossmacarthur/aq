use std::fmt;
use std::str;

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    Json,
    Toml,
    Yaml,
}

impl Default for Format {
    fn default() -> Self {
        Self::Json
    }
}

#[derive(Debug, Error)]
#[error("failed to parse format from `{0}`")]
pub struct ParseFormatError(String);

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Json => "json",
            Self::Toml => "toml",
            Self::Yaml => "yaml",
        };
        f.write_str(s)
    }
}

impl str::FromStr for Format {
    type Err = ParseFormatError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "j" | "json" => Self::Json,
            "t" | "toml" => Self::Toml,
            "y" | "yaml" => Self::Yaml,
            _ => return Err(ParseFormatError(s.to_owned())),
        })
    }
}
