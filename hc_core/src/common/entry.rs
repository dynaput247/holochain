use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash as _Hash, Hasher};

#[derive(Clone, Debug, PartialEq, Hash)]
pub struct Entry {
    content: String,
    hash: u64,
}

impl Entry {
    pub fn new (content: &String) -> Entry {
        let mut e = Entry {
            content: content.clone(),
            hash: 0,
        };
        let mut hasher = DefaultHasher::new();
        _Hash::hash(&e, &mut hasher);
        e.hash = hasher.finish();
        e
    }

    pub fn hash(&self) -> u64 {
        self.hash
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Header {
    previous: Option<u64>,
    entry: u64,
    hash: u64,
}

impl _Hash for Header {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.previous.hash(state);
        self.entry.hash(state);
    }
}

impl Header {
    pub fn new(previous: Option<u64>, entry: &Entry) -> Header {
        let mut h = Header {
            previous: previous,
            entry: entry.hash(),
            hash: 0,
        };
        let mut hasher = DefaultHasher::new();
        _Hash::hash(&h, &mut hasher);
        h.hash = hasher.finish();
        h
    }

    pub fn hash(&self) -> u64 {
        self.hash
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hash {}
