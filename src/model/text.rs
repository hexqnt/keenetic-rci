use std::{fmt, marker::PhantomData, str::FromStr};

use serde::{Deserializer, de::Visitor};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum InvalidInlineAscii {
    Length(usize),
    Character(usize),
}

#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct InlineAscii<const CAPACITY: usize> {
    bytes: [u8; CAPACITY],
    len: u8,
}

impl<const CAPACITY: usize> InlineAscii<CAPACITY> {
    pub(super) fn digits(value: &str, min_len: usize) -> Result<Self, InvalidInlineAscii> {
        Self::parse(value, min_len, u8::is_ascii_digit)
    }

    pub(super) fn uppercase(value: &str) -> Result<Self, InvalidInlineAscii> {
        Self::parse(value, CAPACITY, u8::is_ascii_uppercase)
    }

    fn parse(
        value: &str,
        min_len: usize,
        predicate: impl Fn(&u8) -> bool,
    ) -> Result<Self, InvalidInlineAscii> {
        let bytes = value.as_bytes();
        if !(min_len..=CAPACITY).contains(&bytes.len()) {
            return Err(InvalidInlineAscii::Length(bytes.len()));
        }
        if let Some(index) = bytes.iter().position(|byte| !predicate(byte)) {
            return Err(InvalidInlineAscii::Character(index));
        }

        let mut encoded = [0; CAPACITY];
        encoded[..bytes.len()].copy_from_slice(bytes);
        Ok(Self {
            bytes: encoded,
            len: u8::try_from(bytes.len()).expect("inline ASCII values are shorter than 256 bytes"),
        })
    }

    pub(super) fn as_str(&self) -> &str {
        std::str::from_utf8(self.as_bytes()).expect("an inline ASCII value contains only ASCII")
    }

    pub(super) fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }

    pub(super) const fn as_array(&self) -> &[u8; CAPACITY] {
        &self.bytes
    }
}

pub(super) struct FromStrVisitor<T> {
    expected: &'static str,
    marker: PhantomData<fn() -> T>,
}

impl<T> FromStrVisitor<T> {
    pub(super) const fn new(expected: &'static str) -> Self {
        Self {
            expected,
            marker: PhantomData,
        }
    }
}

impl<T> Visitor<'_> for FromStrVisitor<T>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    type Value = T;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.expected)
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        value.parse().map_err(E::custom)
    }
}

struct OptionalFromStrVisitor<T>(PhantomData<fn() -> T>);

impl<'de, T> Visitor<'de> for OptionalFromStrVisitor<T>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    type Value = Option<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("null, an empty string, or a recognized string value")
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(OptionalFromStrStringVisitor(PhantomData))
    }
}

struct OptionalFromStrStringVisitor<T>(PhantomData<fn() -> T>);

impl<T> Visitor<'_> for OptionalFromStrStringVisitor<T>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    type Value = Option<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an empty string or a recognized string value")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if value.is_empty() {
            Ok(None)
        } else {
            value.parse().map(Some).map_err(E::custom)
        }
    }
}

pub(super) fn deserialize_optional_from_str<'de, D, T>(
    deserializer: D,
) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: fmt::Display,
{
    deserializer.deserialize_option(OptionalFromStrVisitor(PhantomData))
}
