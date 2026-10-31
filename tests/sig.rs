//! `clax-ed25519 v2` (src/sig.rs) held to Clax's own vector (clax main d7812795 crates/clax-core/src/sig.rs `sdk_vector_is_fixed`,
//! mirrored by web/sdk/src/sign.test.ts; via work-clax-sdk-kit-goal 0bd728cb): the canonical string's lines, the agent record and
//! the header. The Ed25519 signature itself is the vector's (a stub signer gives its bytes): fold-core signs nothing until it has an
//! Ed25519 crate, and then this vector's `s` is what that signer must reproduce from seed [0x42; 32].

use fold_core::report::{ClaxMinter, Minter};
use fold_core::sig::{agent_record, header, sha256_hex, SignInput, Signer, VERSION};

const BODY: &[u8] = br#"{"body":"hi"}"#;
const NONCE: &str = "AAECAwQFBgcICQoLDA0ODw";
const S: &str = "8DwcAbZkGf5Qhq47ATzagkKAuIr-T4h6vKczyevVcv1UchPiD_ba5rmjr8HyInFyYULBDia1-PZesVfaOyxtBA";
const NO_AGENT: &str = "a00d0d823baf03fa53cba1260dd2f71fc7e8694679ce444737a7168dfab351e8";

fn vector() -> SignInput<'static> {
    SignInput {
        method: "POST",
        target: "/api/queues/q1/reporter/reports?cursor=c1",
        host: "clax.example:8443",
        t: 1_760_000_000,
        nonce: NONCE,
        body: BODY,
        token: "cxr_vector",
        agent: &[],
    }
}

/// Unpadded base64url, decoded.
fn unb64url(s: &str) -> Vec<u8> {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let (mut out, mut acc, mut bits) = (vec![], 0u32, 0);
    for c in s.bytes() {
        acc = acc << 6 | A.iter().position(|a| *a == c).unwrap() as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

/// A signer that gives the vector's signature: what the real Ed25519 signer must give for the vector's key and string.
struct Vector;

impl Signer for Vector {
    fn sign(&self, msg: &[u8]) -> Result<[u8; 64], String> {
        assert_eq!(std::str::from_utf8(msg).unwrap(), vector().canonical().unwrap(), "signed the canonical string, byte for byte");
        unb64url(S).try_into().map_err(|_| "not 64 bytes".to_string())
    }
}

#[test]
fn the_canonical_string_is_the_vectors() {
    let c = vector().canonical().unwrap();
    let lines: Vec<&str> = c.split('\n').collect();
    assert_eq!(lines.len(), 9, "nine lines, no trailing newline: {c:?}");
    assert_eq!(
        lines,
        [
            VERSION,
            "POST",
            "/api/queues/q1/reporter/reports?cursor=c1",
            "clax.example:8443",
            "1760000000",
            NONCE,
            "8f1de4697a700dd8df293b4b51025bff262ca9f65a2af030cd50acb3818eebfd",
            "1c2cec895430450c4ba5de411b6e3fa57ec583b47af14a0901bb47d3743e9ab1",
            NO_AGENT,
        ]
    );
    assert_eq!(VERSION, "clax-ed25519 v2");
    assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "an empty body hashes the empty string");
    // the agent record: each absent header a `-` line; a present one `+<len>:<trimmed value>`
    assert_eq!(agent_record(&[]), "-\n-\n-\n-\n-\n");
    assert_eq!(sha256_hex(agent_record(&[]).as_bytes()), NO_AGENT);
    assert_eq!(agent_record(&[("X-Clax-Via", " web "), ("x-clax-session", "s1")]), "+2:s1\n-\n+3:web\n-\n-\n");
    // a control character where a line would break is refused
    for (m, p, h, n) in [("PO\nST", "/", "h", NONCE), ("POST", "/a\r\nb", "h", NONCE), ("POST", "/", "h\n", NONCE), ("POST", "/", "h", "a\tb")] {
        let i = SignInput { method: m, target: p, host: h, nonce: n, ..vector() };
        assert!(i.canonical().is_err(), "{m:?} {p:?} {h:?} {n:?}");
    }
}

#[test]
fn the_header_is_the_vectors() {
    assert_eq!(unb64url(S).len(), 64);
    assert_eq!(header(&Vector, &vector()).unwrap(), format!("t=1760000000, n={NONCE}, s={S}"));
}

#[test]
fn the_minter_signs_its_own_request_with_the_host_it_sends() {
    struct Echo;
    impl Signer for Echo {
        fn sign(&self, msg: &[u8]) -> Result<[u8; 64], String> {
            let c = std::str::from_utf8(msg).unwrap();
            let lines: Vec<&str> = c.split('\n').collect();
            assert_eq!(&lines[..6], ["clax-ed25519 v2", "POST", "/api/queues/basis/reporter-tokens", "127.0.0.1:7480", "1760000000", NONCE]);
            assert_eq!(lines[7], sha256_hex(b"cxq_TESTONLY"), "the grant's token, by its hash only");
            Ok([7; 64])
        }
    }
    let m = ClaxMinter::new("http://127.0.0.1:7480".into(), "basis".into(), "cxq_TESTONLY".into(), "k".into());
    let (path, _, body) = m.request("PK0123456789abcdef", "https://phobos.example.ts.net", "bobby@example.com");
    // without a signer: refused, before any connection
    assert!(m.signature(&path, &body, 1_760_000_000, NONCE).unwrap_err().contains("not built"));
    let m = m.with_signer(Box::new(Echo));
    let h = m.signature(&path, &body, 1_760_000_000, NONCE).unwrap();
    assert!(h.starts_with(&format!("t=1760000000, n={NONCE}, s=")) && h.len() == "t=1760000000, n=, s=".len() + 22 + 86, "{h}");
    // with a signer but no instance listening (port 9, never a real Clax): the call fails, said without the grant
    struct Any;
    impl Signer for Any {
        fn sign(&self, _: &[u8]) -> Result<[u8; 64], String> {
            Ok([7; 64])
        }
    }
    let dead = ClaxMinter::new("http://127.0.0.1:9".into(), "basis".into(), "cxq_TESTONLY".into(), "k".into()).with_signer(Box::new(Any));
    let e = dead.mint("PK0123456789abcdef", "https://phobos.example.ts.net", "bobby@example.com").unwrap_err();
    assert!(!e.contains("cxq_TESTONLY"), "{e}");
}
