//! Lazy, reactive evaluation keyed by node identity (docs/DESIGN-lazy-reactive.md). Behind the `reactive` feature; nothing
//! in the fold path calls it yet.
//!
//! A node is a key (its stable id) and a pure function of what it reads: input versions (facto vertices, by id) and other
//! nodes (by key). `Memo` remembers, for each key, the exact (id, version) pairs the last evaluation read and a version of
//! its own output, and keeps the reverse index (who read me). Nothing is evaluated until `get`; a change feed marks readers
//! dirty by id; a dirty node is revalidated by comparing what it read with what is current and re-evaluated only when
//! something it read really changed. When a re-evaluation yields the same output its version does not move, so the nodes
//! above it are revalidated, not recomputed (early cutoff). Pure and deterministic: no clock, no I/O, no pid, no path.

use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// What one evaluation read: an input or a node, and the version it saw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Read {
    pub id: String,
    pub version: String,
}

/// The pure function of a node. It reads through the `Reader` so the reads are recorded; it must read nothing else.
pub trait Eval {
    fn eval(&self, key: &str, r: &mut Reader) -> Result<String, String>;
}

struct Entry {
    reads: Vec<Read>,
    out: String,
    version: String,
    dirty: bool,
}

/// What `Memo::feed` did with one batch of the facto change feed.
#[derive(Debug, PartialEq, Eq)]
pub enum Fed {
    /// `seq` was the next one: these nodes (readers, transitively) are dirty, in key order.
    Applied(Vec<String>),
    /// `seq` was already applied (a redelivery): nothing changed.
    Duplicate,
    /// `seq` skipped ahead: the feed lost changes, so everything is dirty and every input version is forgotten.
    Gap { from: u64, to: u64 },
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct Stats {
    pub evals: u64,
    pub hits: u64,
    pub revalidated: u64,
}

#[derive(Default)]
pub struct Memo {
    seq: u64,
    inputs: BTreeMap<String, String>,
    entries: BTreeMap<String, Entry>,
    /// input id or node key -> the nodes that read it at their last evaluation
    readers: BTreeMap<String, BTreeSet<String>>,
    pub stats: Stats,
}

pub struct Reader<'a> {
    memo: &'a mut Memo,
    ev: &'a dyn Eval,
    stack: &'a mut Vec<String>,
    reads: Vec<Read>,
}

/// Version of an output: the first 16 hex of its sha-256, so equal outputs have equal versions whatever made them.
pub fn version_of(out: &str) -> String {
    Sha256::digest(out.as_bytes()).iter().take(8).map(|b| format!("{b:02x}")).collect()
}

impl Reader<'_> {
    /// Read an input (a facto vertex) at the version the caller fetched it at; returns nothing, the data is the caller's.
    pub fn input(&mut self, id: &str, version: &str) {
        self.memo.inputs.insert(id.to_string(), version.to_string());
        self.reads.push(Read { id: id.to_string(), version: version.to_string() });
    }

    /// Read another node (lazily evaluated now, if it is not valid) and get its output.
    pub fn node(&mut self, key: &str) -> Result<String, String> {
        let (out, version) = self.memo.get_in(key, self.ev, self.stack)?;
        self.reads.push(Read { id: key.to_string(), version });
        Ok(out)
    }
}

impl Memo {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// Read a node: its recorded output when valid, else revalidated, else evaluated. Errors are not cached.
    pub fn get(&mut self, key: &str, ev: &dyn Eval) -> Result<String, String> {
        self.get_in(key, ev, &mut Vec::new()).map(|(out, _)| out)
    }

    /// The nodes that read `id` at their last evaluation (the reverse index), in key order.
    pub fn readers_of(&self, id: &str) -> Vec<String> {
        self.readers.get(id).map(|s| s.iter().cloned().collect()).unwrap_or_default()
    }

    /// Apply one batch of the change feed: `seq` is the graph's change_seq after the batch; `changed` the (id, new version)
    /// pairs. Marks readers dirty (transitively); evaluates nothing.
    pub fn feed(&mut self, seq: u64, changed: &[(&str, &str)]) -> Fed {
        if seq <= self.seq && self.seq != 0 {
            return Fed::Duplicate;
        }
        if self.seq != 0 && seq != self.seq + 1 {
            let gap = Fed::Gap { from: self.seq, to: seq };
            self.seq = seq;
            self.inputs.clear();
            for e in self.entries.values_mut() {
                e.dirty = true;
            }
            return gap;
        }
        self.seq = seq;
        let mut dirty = BTreeSet::new();
        let mut work: Vec<String> = Vec::new();
        for (id, version) in changed {
            if self.inputs.get(*id).map(|v| v.as_str()) != Some(*version) {
                self.inputs.insert(id.to_string(), version.to_string());
                work.push(id.to_string());
            }
        }
        while let Some(id) = work.pop() {
            for k in self.readers.get(&id).into_iter().flatten() {
                if dirty.insert(k.clone()) {
                    work.push(k.clone());
                }
            }
        }
        for k in &dirty {
            if let Some(e) = self.entries.get_mut(k) {
                e.dirty = true;
            }
        }
        Fed::Applied(dirty.into_iter().collect())
    }

    fn get_in(&mut self, key: &str, ev: &dyn Eval, stack: &mut Vec<String>) -> Result<(String, String), String> {
        if stack.iter().any(|k| k == key) {
            return Err(format!("cycle: {} -> {key}", stack.join(" -> ")));
        }
        stack.push(key.to_string());
        let r = self.get_checked(key, ev, stack);
        stack.pop();
        r
    }

    fn get_checked(&mut self, key: &str, ev: &dyn Eval, stack: &mut Vec<String>) -> Result<(String, String), String> {
        if let Some(e) = self.entries.get(key) {
            if !e.dirty {
                self.stats.hits += 1;
                return Ok((e.out.clone(), e.version.clone()));
            }
            // Revalidate: bring every node it read up to date first (they may cut off), then compare versions.
            let reads = e.reads.clone();
            let mut same = true;
            for r in &reads {
                let now = if self.entries.contains_key(&r.id) {
                    self.get_in(&r.id, ev, stack)?.1
                } else {
                    self.inputs.get(&r.id).cloned().unwrap_or_default()
                };
                if now != r.version {
                    same = false;
                    break;
                }
            }
            if same {
                let e = self.entries.get_mut(key).unwrap();
                e.dirty = false;
                self.stats.revalidated += 1;
                return Ok((e.out.clone(), e.version.clone()));
            }
        }
        self.stats.evals += 1;
        let mut rd = Reader { memo: self, ev, stack, reads: Vec::new() };
        let out = ev.eval(key, &mut rd)?;
        let reads = std::mem::take(&mut rd.reads);
        if let Some(old) = self.entries.remove(key) {
            for r in &old.reads {
                if let Some(s) = self.readers.get_mut(&r.id) {
                    s.remove(key);
                }
            }
        }
        for r in &reads {
            self.readers.entry(r.id.clone()).or_default().insert(key.to_string());
        }
        let version = version_of(&out);
        self.entries.insert(key.to_string(), Entry { reads, out: out.clone(), version: version.clone(), dirty: false });
        Ok((out, version))
    }
}
