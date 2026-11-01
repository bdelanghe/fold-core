//! The fold contract (src/check.rs): fold-core's own pages pass; a hand-made page in the viewer's chrome fails; documents with
//! missing labels, missing item ids or repeated sibling ids fail with their paths.

use fold_core::check;
use serde_json::json;

#[test]
fn fold_cores_own_page_carries_the_stamp_and_passes() {
    let tree = json!({"role": "main", "children": [{"role": "heading", "name": "T", "level": 1}]});
    if let Ok(html) = fold_core::page::page(&tree, "/fold/") {
        assert!(html.contains(check::GENERATOR));
        assert_eq!(check::page(&html), Ok(()));
    }
    let stamped = format!("<head>{}<link rel=\"stylesheet\" href=\"/fold/fold.css\"></head>", check::GENERATOR);
    assert_eq!(check::page(&stamped), Ok(()));
}

#[test]
fn a_hand_made_page_in_the_fold_chrome_fails() {
    let fake = r#"<head><link rel="stylesheet" href="/fold/fold.css"><meta name="apple-mobile-web-app-title" content="Basis"></head><main><nav aria-label="Path"><a href="/fold/">Everything</a> › TRMNL</nav><form>…</form></main>"#;
    assert!(check::page(fake).is_err());
    assert_eq!(check::page("<html><body>not a fold, and doesn't say so</body></html>"), Ok(()));
}

#[test]
fn a_good_document_passes_and_nested_sections_are_fine() {
    let d = json!({"title": "Kitchen", "sections": [
        {"id": "fridge", "title": "Fridge", "groups": [{"id": "crisper", "title": "Crisper", "items": [{"id": "a", "name": "Apple"}]}]},
        {"id": "fleet", "title": "Fleet", "sections": [{"id": "x", "title": "X", "groups": []}]}]});
    assert_eq!(check::doc(&d), Ok(()));
}

#[test]
fn bad_documents_fail_with_paths() {
    let d = json!({"sections": [
        {"id": "s", "title": "S", "groups": [{"title": "G", "items": [{"name": "no id"}, {"id": "dup", "name": "a"}, {"id": "dup", "name": "b"}, {"id": "q"}]}]},
        {"id": "s", "title": "again"}]});
    let errs = check::doc(&d).unwrap_err();
    let all = errs.join("\n");
    assert!(all.contains("/title: missing"), "{all}");
    assert!(all.contains("/sections/0/groups/0/items/0/id: missing"), "{all}");
    assert!(all.contains("/sections/0/groups/0/items/2/id: \"dup\" repeats"), "{all}");
    assert!(all.contains("/sections/0/groups/0/items/3: no label"), "{all}");
    assert!(all.contains("/sections/1/id: \"s\" repeats"), "{all}");
    assert!(check::doc(&json!({"title": "empty"})).is_err());
}
