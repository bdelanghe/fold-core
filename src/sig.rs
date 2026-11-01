//! `clax-ed25519 v2`, the request signature a Clax instance takes from a service caller (report.rs's mint), written here from its
//! description and held to its own test vector (clax main d7812795 crates/clax-core/src/sig.rs `SignInput::canonical`,
//! `sdk_vector_is_fixed`; via work-clax-sdk-kit-goal 0bd728cb). Reimplemented rather than depended on: clax-core's licence
//! (GPL-2.0-only OR AGPL-3.0-only for Clax's shared crates) is not fold-core's MIT.
//!
//! The canonical string is nine lines joined by `\n`, with no trailing newline: the version, the method, the path and query as sent,
//! the Host as sent, `t` (Unix seconds), the nonce (unpadded base64url of 16 random bytes), and the lowercase-hex SHA-256 of the
//! body, of the token, and of the agent headers' record (`-\n` for each absent one; a queue request carries none). The header is
//! `t=<t>, n=<nonce>, s=<signature>`, the 64-byte Ed25519 signature in unpadded base64url.
//!
//! The Ed25519 signing itself is a [`Signer`]; the real one, [`Ed25519`], is ed25519-dalek 2.2 (BSD-3-Clause, native only; added by
//! personal-fold-core-goal in 37b51be), made from the grant's PKCS#8 key, and held to the vector's `s` in tests/sig.rs.

use sha2::{Digest, Sha256};

use crate::act::b64url;

/// The version line.
pub const VERSION: &str = "clax-ed25519 v2";

/// The agent headers, in their order: a queue request carries none of them.
pub const AGENT_HEADERS: [&str; 5] = ["x-clax-session", "x-clax-channel", "x-clax-via", "x-clax-git", "x-clax-call"];

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Lowercase-hex SHA-256.
pub fn sha256_hex(b: &[u8]) -> String {
    hex(&Sha256::digest(b))
}

/// The agent headers' record: each in [`AGENT_HEADERS`] order, absent as `-`, present as `+<byte length>:<value>` (trimmed), each
/// ending in `\n`.
pub fn agent_record(headers: &[(&str, &str)]) -> String {
    let mut o = String::new();
    for name in AGENT_HEADERS {
        match headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.trim()) {
            Some(v) => o.push_str(&format!("+{}:{v}\n", v.len())),
            None => o.push_str("-\n"),
        }
    }
    o
}

/// One request as it is signed.
pub struct SignInput<'a> {
    pub method: &'a str,
    /// The path and query exactly as sent.
    pub target: &'a str,
    /// The Host header exactly as sent, port and all.
    pub host: &'a str,
    pub t: u64,
    pub nonce: &'a str,
    pub body: &'a [u8],
    /// The `cxq_` or `cxr_` token the request carries.
    pub token: &'a str,
    pub agent: &'a [(&'a str, &'a str)],
}

impl SignInput<'_> {
    /// The canonical string, or why it cannot be one (a control character where a line would break).
    pub fn canonical(&self) -> Result<String, String> {
        for (what, s) in [("method", self.method), ("path", self.target), ("host", self.host), ("nonce", self.nonce)] {
            if s.chars().any(char::is_control) {
                return Err(format!("clax-ed25519: a control character in the {what}"));
            }
        }
        Ok([
            VERSION.to_string(),
            self.method.to_string(),
            self.target.to_string(),
            self.host.to_string(),
            self.t.to_string(),
            self.nonce.to_string(),
            sha256_hex(self.body),
            sha256_hex(self.token.as_bytes()),
            sha256_hex(agent_record(self.agent).as_bytes()),
        ]
        .join("\n"))
    }
}

/// What makes an Ed25519 signature over a message, with the grant's key.
pub trait Signer: Send + Sync {
    fn sign(&self, msg: &[u8]) -> Result<[u8; 64], String>;
}

/// The `Clax-Signature` header's value for a request.
pub fn header(signer: &dyn Signer, input: &SignInput<'_>) -> Result<String, String> {
    let sig = signer.sign(input.canonical()?.as_bytes())?;
    Ok(format!("t={}, n={}, s={}", input.t, input.nonce, b64url(&sig)))
}

/// Unpadded (or padded) base64url, decoded; None for anything else.
pub fn unb64url(s: &str) -> Option<Vec<u8>> {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let (mut out, mut acc, mut bits) = (vec![], 0u32, 0u32);
    for c in s.trim_end_matches('=').bytes() {
        acc = (acc << 6 | A.iter().position(|a| *a == c)? as u32) & 0xFFFF;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// The real signer: the grant's Ed25519 key (ed25519-dalek, BSD-3-Clause). No Debug, so it cannot be printed.
pub struct Ed25519(ed25519_dalek::SigningKey);

impl Ed25519 {
    /// From the grant's private key as Clax gives it: PKCS#8 DER, unpadded base64url. Why not, without the key in it.
    pub fn from_pkcs8_b64url(key: &str) -> Result<Ed25519, String> {
        use ed25519_dalek::pkcs8::DecodePrivateKey;
        let der = unb64url(key.trim()).ok_or("the grant's key is not base64url")?;
        ed25519_dalek::SigningKey::from_pkcs8_der(&der).map(Ed25519).map_err(|_| "the grant's key is not a PKCS#8 Ed25519 key".to_string())
    }

    /// From a 32-byte seed (Clax's test vector uses [0x42; 32]).
    pub fn from_seed(seed: &[u8; 32]) -> Ed25519 {
        Ed25519(ed25519_dalek::SigningKey::from_bytes(seed))
    }

    /// The public key, unpadded base64url, as Clax writes one.
    pub fn public_b64url(&self) -> String {
        b64url(&self.0.verifying_key().to_bytes())
    }
}

impl Signer for Ed25519 {
    fn sign(&self, msg: &[u8]) -> Result<[u8; 64], String> {
        use ed25519_dalek::Signer as _;
        Ok(self.0.sign(msg).to_bytes())
    }
}

/// A fresh nonce: 16 bytes from the system's random source, unpadded base64url (22 characters).
pub fn nonce() -> Result<String, String> {
    use std::io::Read;
    let mut b = [0u8; 16];
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).map_err(|e| format!("no random source: {e}"))?;
    Ok(b64url(&b))
}
