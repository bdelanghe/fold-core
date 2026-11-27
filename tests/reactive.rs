//! Lazy, reactive evaluation by node identity (src/reactive.rs): a scenario against goldens/reactive/needs.log. FOLD_BLESS=1 rewrites.
#![cfg(feature = "reactive")]

use fold_core::reactive::{Eval, Fed, Memo, Reader};
use std::cell::RefCell;
use std::collections::BTreeMap;

/// A tiny Needs-you: `needs` reads the `approvals` and `asks` nodes; each reads facto vertices (id -> (version, value)).
struct Needs {
    vertices: RefCell<BTreeMap<String, (u64, String)>>,
    log: RefCell<Vec<String>>,
}

impl Needs {
    fn set(&self, id: &str, value: &str) -> (String, String) {
        let mut v = self.vertices.borrow_mut();
        let version = v.get(id).map_or(1, |(n, _)| n + 1);
        v.insert(id.to_string(), (version, value.to_string()));
        (id.to_string(), version.to_string())
    }
    fn vertex(&self, id: &str, r: &mut Reader) -> String {
        let (n, value) = self.vertices.borrow()[id].clone();
        r.input(id, &n.to_string());
        value
    }
}

impl Eval for Needs {
    fn eval(&self, key: &str, r: &mut Reader) -> Result<String, String> {
        self.log.borrow_mut().push(format!("eval {key}"));
        Ok(match key {
            // only the ones still waiting are shown, so a vertex going waiting -> done -> done changes nothing above
            "needs/approvals" => ["v:a1", "v:a2"]
                .iter()
                .map(|id| (id, self.vertex(id, r)))
                .filter(|(_, s)| s == "waiting")
                .map(|(id, _)| id.to_string())
                .collect::<Vec<_>>()
                .join(","),
            "needs/asks" => self.vertex("v:q1", r),
            "needs" => format!("approvals=[{}] asks=[{}]", r.node("needs/approvals")?, r.node("needs/asks")?),
            "loop/a" => r.node("loop/b")?,
            "loop/b" => r.node("loop/a")?,
            _ => return Err(format!("no node {key}")),
        })
    }
}

fn step(out: &mut String, label: &str, n: &Needs, m: &mut Memo, key: &str) {
    n.log.borrow_mut().clear();
    let got = m.get(key, n);
    let evals = n.log.borrow().join(", ");
    out.push_str(&format!(
        "{label}: {key} -> {got:?}\n  evals [{evals}] total evals={} hits={} revalidated={}\n",
        m.stats.evals, m.stats.hits, m.stats.revalidated
    ));
}

#[test]
fn only_what_is_read_and_what_changed_is_evaluated() {
    let n = Needs { vertices: RefCell::new(BTreeMap::new()), log: RefCell::new(Vec::new()) };
    let mut m = Memo::new();
    let mut out = String::new();
    for (id, v) in [("v:a1", "waiting"), ("v:a2", "done"), ("v:q1", "open")] {
        n.set(id, v);
    }

    step(&mut out, "1 cold read of one leaf is lazy", &n, &mut m, "needs/asks");
    step(&mut out, "2 cold read of the root reads the rest", &n, &mut m, "needs");
    step(&mut out, "3 warm read evaluates nothing", &n, &mut m, "needs");
    out.push_str(&format!("readers of v:a1 {:?}; of needs/approvals {:?}\n", m.readers_of("v:a1"), m.readers_of("needs/approvals")));

    // a2 changes waiting-irrelevant data (done -> done, new version): approvals re-evaluates, its output is the same, needs is only revalidated
    let (id, ver) = n.set("v:a2", "done");
    out.push_str(&format!("4 feed seq 1 {id}@{ver}: {:?}\n", m.feed(1, &[(&id, &ver)])));
    step(&mut out, "4 read after a cutoff", &n, &mut m, "needs");

    // a1 changes for real: approvals and needs re-evaluate, asks is a hit
    let (id, ver) = n.set("v:a1", "done");
    out.push_str(&format!("5 feed seq 2 {id}@{ver}: {:?}\n", m.feed(2, &[(&id, &ver)])));
    step(&mut out, "5 read after a real change", &n, &mut m, "needs");

    out.push_str(&format!("6 redelivered seq 2: {:?}\n", m.feed(2, &[("v:a1", "2")])));
    step(&mut out, "6 read after a redelivery", &n, &mut m, "needs");

    // seq 3 never arrives; seq 4 does: the feed lost changes, so nothing is trusted and everything is re-read lazily
    n.set("v:q1", "answered");
    out.push_str(&format!("7 feed seq 4 after 2: {:?}\n", m.feed(4, &[])));
    step(&mut out, "7 read after a gap", &n, &mut m, "needs");
    step(&mut out, "7 and again", &n, &mut m, "needs");

    step(&mut out, "8 a cycle is an error, not a hang", &n, &mut m, "loop/a");
    step(&mut out, "8 an unknown node is an error", &n, &mut m, "nope");

    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/goldens/reactive/needs.log");
    if std::env::var("FOLD_BLESS").is_ok() {
        std::fs::create_dir_all(concat!(env!("CARGO_MANIFEST_DIR"), "/goldens/reactive")).unwrap();
        std::fs::write(path, &out).unwrap();
    }
    assert_eq!(out, std::fs::read_to_string(path).expect("run with FOLD_BLESS=1 once"));
    assert!(matches!(m.feed(9, &[]), Fed::Gap { from: 4, to: 9 }));
}
