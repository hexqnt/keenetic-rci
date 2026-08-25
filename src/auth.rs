use std::fmt;

use md5::{Digest as _, Md5};
use serde::Serialize;
use sha2::Sha256;
use zeroize::Zeroizing;

pub struct Secret(Zeroizing<String>);

impl Secret {
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Secret([REDACTED])")
    }
}

pub struct Credentials {
    pub login: Box<str>,
    pub password: Secret,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Credentials")
            .field("login", &self.login)
            .field("password", &self.password)
            .finish()
    }
}

#[derive(Serialize)]
pub struct AuthPayload<'a> {
    pub login: &'a str,
    pub password: &'a str,
}

pub struct ResponseHash(Zeroizing<[u8; 64]>);

impl ResponseHash {
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(self.0.as_ref())
            .expect("a response hash contains only lowercase ASCII hexadecimal digits")
    }
}

fn encode_lower_hex(bytes: &[u8], encoded: &mut [u8]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    debug_assert_eq!(encoded.len(), bytes.len() * 2);
    let (pairs, remainder) = encoded.as_chunks_mut::<2>();
    debug_assert_eq!(remainder, b"");
    for (&byte, pair) in bytes.iter().zip(pairs) {
        pair[0] = HEX[usize::from(byte >> 4)];
        pair[1] = HEX[usize::from(byte & 0x0f)];
    }
}

pub fn response_hash(login: &str, realm: &str, password: &Secret, challenge: &str) -> ResponseHash {
    let mut md5 = Md5::new();
    md5.update(login.as_bytes());
    md5.update(b":");
    md5.update(realm.as_bytes());
    md5.update(b":");
    md5.update(password.expose().as_bytes());
    let md5_digest = md5.finalize();
    let mut md5_hex = Zeroizing::new([0; 32]);
    encode_lower_hex(&md5_digest, md5_hex.as_mut());

    let mut sha256 = Sha256::new();
    sha256.update(challenge.as_bytes());
    sha256.update(md5_hex.as_ref());
    let sha256_digest = sha256.finalize();
    let mut encoded = Zeroizing::new([0; 64]);
    encode_lower_hex(&sha256_digest, encoded.as_mut());
    ResponseHash(encoded)
}

#[cfg(test)]
mod tests {
    use super::{Secret, response_hash};

    #[test]
    fn known_challenge_response_vector() {
        let result = response_hash(
            "admin",
            "Keenetic Ultra",
            &Secret::new("secret".into()),
            "0123456789abcdef0123456789abcdef",
        );
        assert_eq!(
            result.as_str(),
            "9071bc0aeee8b97dfe926020c8aaeb35951c0f67998f1027eea5465643bc3632"
        );
    }

    #[test]
    fn secret_debug_is_redacted() {
        let secret = Secret::new("unique-password".into());
        let debug = format!("{secret:?}");
        assert!(!debug.contains("unique-password"));
    }
}
