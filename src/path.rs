use std::{fmt, str::FromStr};

use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, utf8_percent_encode};
use thiserror::Error;

const SEGMENT_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'!')
    .add(b'"')
    .add(b'#')
    .add(b'$')
    .add(b'%')
    .add(b'&')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

/// Namespace of a validated relative path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PathKind {
    /// A JSON or text endpoint below `/rci/`.
    Rci,
    /// A configuration text endpoint below `/ci/`.
    Ci,
}

impl fmt::Display for PathKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Rci => "RCI",
            Self::Ci => "CI",
        })
    }
}

/// A relative RCI/CI path failed canonical validation.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("invalid {kind} path: {reason}")]
pub struct InvalidPath {
    kind: PathKind,
    reason: &'static str,
}

impl InvalidPath {
    const fn new(kind: PathKind, reason: &'static str) -> Self {
        Self { kind, reason }
    }

    /// Returns the path namespace.
    #[must_use]
    pub const fn kind(&self) -> PathKind {
        self.kind
    }

    /// Returns a non-sensitive explanation.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        self.reason
    }
}

macro_rules! path_type {
    ($name:ident, $kind:expr, $docs:literal) => {
        #[doc = $docs]
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(Box<str>);

        impl $name {
            /// Returns the canonical percent-encoded relative path.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl FromStr for $name {
            type Err = InvalidPath;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                parse(value, $kind).map(Self)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }
    };
}

path_type!(
    RciPath,
    PathKind::Rci,
    "A validated, canonical relative path below `/rci/`."
);
path_type!(
    CiPath,
    PathKind::Ci,
    "A validated, canonical relative path below `/ci/`."
);

fn parse(value: &str, kind: PathKind) -> Result<Box<str>, InvalidPath> {
    if value.is_empty() {
        return Err(InvalidPath::new(kind, "the path is empty"));
    }
    if value.starts_with('/') {
        return Err(InvalidPath::new(kind, "the path must be relative"));
    }
    if value.contains(['?', '#']) {
        return Err(InvalidPath::new(
            kind,
            "query strings and fragments are forbidden",
        ));
    }
    if value.contains('\\') {
        return Err(InvalidPath::new(kind, "backslashes are forbidden"));
    }
    if value.contains("://") {
        return Err(InvalidPath::new(kind, "schemes and hosts are forbidden"));
    }
    let namespace_prefix = match kind {
        PathKind::Rci => "rci/",
        PathKind::Ci => "ci/",
    };
    if value.starts_with(namespace_prefix) {
        return Err(InvalidPath::new(
            kind,
            "the namespace prefix must be omitted",
        ));
    }

    let mut canonical = String::with_capacity(value.len());
    for (index, segment) in value.split('/').enumerate() {
        if segment.is_empty() {
            return Err(InvalidPath::new(kind, "empty path segments are forbidden"));
        }
        validate_percent_encoding(segment).map_err(|reason| InvalidPath::new(kind, reason))?;
        let decoded = percent_decode_str(segment)
            .decode_utf8()
            .map_err(|_| InvalidPath::new(kind, "segments must be valid UTF-8"))?;
        if decoded.is_empty() || decoded == "." || decoded == ".." {
            return Err(InvalidPath::new(
                kind,
                "empty, `.` and `..` segments are forbidden",
            ));
        }
        if decoded.contains(['/', '\\']) {
            return Err(InvalidPath::new(
                kind,
                "encoded path separators are forbidden",
            ));
        }
        if decoded.chars().any(char::is_control) {
            return Err(InvalidPath::new(kind, "control characters are forbidden"));
        }
        if index != 0 {
            canonical.push('/');
        }
        canonical.extend(utf8_percent_encode(&decoded, SEGMENT_ENCODE_SET));
    }
    if canonical.starts_with(namespace_prefix) {
        return Err(InvalidPath::new(
            kind,
            "the namespace prefix must be omitted",
        ));
    }
    Ok(canonical.into_boxed_str())
}

const fn validate_percent_encoding(value: &str) -> Result<(), &'static str> {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return Err("percent escapes must contain two hexadecimal digits");
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::RciPath;

    #[test]
    fn canonicalizes_safe_percent_encoding() {
        let path: RciPath = "show/%69nterface-name".parse().unwrap();
        assert_eq!(path.as_str(), "show/interface-name");
    }

    #[test]
    fn rejects_ambiguous_paths() {
        for invalid in [
            "",
            "/show",
            "show//version",
            "show/.",
            "show/..",
            "show/%2e",
            "show/%2E%2e",
            "show/%2fversion",
            "show/%5Cversion",
            "show/%00version",
            "show\\version",
            "show?x=1",
            "show#x",
            "http://router/rci/show",
            "rci/show/version",
            "r%63i/show/version",
        ] {
            assert!(invalid.parse::<RciPath>().is_err(), "accepted {invalid:?}");
        }
    }
}
