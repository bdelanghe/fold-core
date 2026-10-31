//! The id rules the pages stand on (src/id.rs): a source id as one percent-encoded segment, the haecceity IRI and the HTML id.
//! The cases are goldens/ids.vectors.json's `segment` and `haecceity` sections as PR #12 has them (vectors 614d45da), which
//! facto's graphdb-ids and Allie check against; #12 brings the whole file and its test back over these.

use fold_core::id::{haecceity, html_id, segment_of, BASE};

#[test]
fn a_source_id_is_one_segment_percent_encoded_never_slugged() {
    for (id, want) in [
        ("needs", "needs"),
        ("2fac998588cf", "2fac998588cf"),
        ("pr/12", "pr%2F12"),
        ("Needs you", "Needs%20you"),
        ("Café — Ünïcode", "Caf%C3%A9%20%E2%80%94%20%C3%9Cn%C3%AFcode"),
        ("日本語", "%E6%97%A5%E6%9C%AC%E8%AA%9E"),
        ("a#3 (draft) [x]", "a%233%20%28draft%29%20%5Bx%5D"),
        ("~tilde", "%7Etilde"),
        ("50%", "50%25"),
        ("UPPER-lower_9.v2", "UPPER-lower_9.v2"),
    ] {
        assert_eq!(segment_of(id), want, "{id:?}");
    }
}

#[test]
fn the_haecceity_and_html_id_carry_the_same_path() {
    assert_eq!(BASE, "https://ipso.bounded.tools/");
    for (source, path, iri, id) in [
        ("fleet", "", "https://ipso.bounded.tools/fleet", "fleet"),
        ("fleet", "needs/2fac998588cf", "https://ipso.bounded.tools/fleet/needs/2fac998588cf", "needs/2fac998588cf"),
        (
            "Fleet Map",
            "prs/bdelanghe%2Ffold-core%2312",
            "https://ipso.bounded.tools/Fleet%20Map/prs/bdelanghe%2Ffold-core%2312",
            "prs/bdelanghe%2Ffold-core%2312",
        ),
    ] {
        assert_eq!(haecceity(source, path), iri);
        assert_eq!(html_id(source, path), id);
    }
}
