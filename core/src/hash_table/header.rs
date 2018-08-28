use chain::{Chain, SourceChain};
use hash;
use hash_table::{entry::Entry, HashString};
use multihash::Hash;

/// Header of a source chain "Item"
/// The hash of the Header is used as the Item's key in the source chain hash table
/// Headers are linked to next header in chain and next header of same type in chain
// @TODO - serialize properties as defined in HeadersEntrySchema from golang alpha 1
// @see https://github.com/holochain/holochain-proto/blob/4d1b8c8a926e79dfe8deaa7d759f930b66a5314f/entry_headers.go#L7
// @see https://github.com/holochain/holochain-rust/issues/75
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Header {
    /// the type of this entry
    /// system types may have associated "subconscious" behavior
    entry_type: String,
    /// ISO8601 time stamp
    timestamp: String,
    /// Key to the immediately preceding header. Only the genesis Pair can have None as valid
    link: Option<HashString>,
    /// Key to the entry of this header
    entry_hash: HashString,
    /// agent's cryptographic signature of the entry
    entry_signature: String,
    /// Key to the most recent header of the same type, None is valid only for the first of that type
    link_same_type: Option<HashString>,
}

impl PartialEq for Header {
    fn eq(&self, other: &Header) -> bool {
        self.hash() == other.hash()
    }
}

impl Header {
    /// build a new Header from a chain, entry type and entry.
    /// a Header is immutable, but the chain is mutable if chain.push() is used.
    /// this means that a header becomes invalid and useless as soon as the chain is mutated
    /// the only valid usage of a header is to immediately push it onto a chain in a Pair.
    /// normally (outside unit tests) the generation of valid headers is internal to the
    /// chain::SourceChain trait and should not need to be handled manually
    ///
    /// @see chain::pair::Pair
    /// @see chain::entry::Entry
    pub fn new(chain: &Chain, entry: &Entry) -> Header {
        Header {
            entry_type: entry.entry_type().clone(),
            // @TODO implement timestamps
            // https://github.com/holochain/holochain-rust/issues/70
            timestamp: String::new(),
            link: chain.top_pair().as_ref().map(|p| p.header().hash()),
            entry_hash: entry.hash().to_string(),
            link_same_type: chain
                .top_pair_type(&entry.entry_type())
                // @TODO inappropriate expect()?
                // @see https://github.com/holochain/holochain-rust/issues/147
                .map(|p| p.header().hash()),
            // @TODO implement signatures
            // https://github.com/holochain/holochain-rust/issues/71
            entry_signature: String::new(),
        }
    }

    /// entry_type getter
    pub fn entry_type(&self) -> &str {
        &self.entry_type
    }
    /// timestamp getter
    pub fn timestamp(&self) -> &str {
        &self.timestamp
    }
    /// link getter
    pub fn link(&self) -> Option<String> {
        self.link.clone()
    }
    /// entry_hash getter
    pub fn entry_hash(&self) -> &str {
        &self.entry_hash
    }
    /// link_same_type getter
    pub fn link_same_type(&self) -> Option<String> {
        self.link_same_type.clone()
    }
    /// entry_signature getter
    pub fn entry_signature(&self) -> &str {
        &self.entry_signature
    }

    /// hashes the header
    pub fn hash(&self) -> String {
        // @TODO this is the wrong string being hashed
        // @see https://github.com/holochain/holochain-rust/issues/103
        let pieces: [&str; 6] = [
            &self.entry_type,
            &self.timestamp,
            &self.link.clone().unwrap_or_default(),
            &self.entry_hash,
            &self.link_same_type.clone().unwrap_or_default(),
            &self.entry_signature,
        ];
        let string_to_hash = pieces.concat();

        // @TODO the hashing algo should not be hardcoded
        // @see https://github.com/holochain/holochain-rust/issues/104
        hash::str_to_b58_hash(&string_to_hash, Hash::SHA2256)
    }

    /// returns true if the header is valid
    pub fn validate(&self) -> bool {
        // always valid iff immutable and new() enforces validity
        true
    }

    /// returns the key for use in hash table lookups, e.g. chain.get()
    pub fn key(&self) -> String {
        self.hash()
    }
}

#[cfg(test)]
mod tests {
    use chain::{tests::test_chain, SourceChain};
    use hash_table::{entry::Entry, header::Header, pair::tests::test_pair};

    /// returns a dummy header for use in tests
    pub fn test_header() -> Header {
        test_pair().header().clone()
    }

    #[test]
    /// tests for PartialEq
    fn eq() {
        let chain1 = test_chain();
        let c1 = "foo";
        let c2 = "bar";
        let t1 = "a";
        let t2 = "b";

        // same content + type + state is equal
        assert_eq!(
            Header::new(&chain1, &Entry::new(t1, c1)),
            Header::new(&chain1, &Entry::new(t1, c1))
        );

        // different content is different
        assert_ne!(
            Header::new(&chain1, &Entry::new(t1, c1)),
            Header::new(&chain1, &Entry::new(t1, c2))
        );

        // different type is different
        assert_ne!(
            Header::new(&chain1, &Entry::new(t1, c1)),
            Header::new(&chain1, &Entry::new(t2, c1)),
        );

        // different state is different
        let mut chain2 = test_chain();
        let e = Entry::new(t1, c1);
        chain2
            .push_entry(&e)
            .expect("pushing a valid entry to an exlusively owned chain shouldn't fail");

        assert_ne!(Header::new(&chain1, &e), Header::new(&chain2, &e));
    }

    #[test]
    /// tests for Header::new()
    fn new() {
        let chain = test_chain();
        let t = "type";
        let e = Entry::new(t, "foo");
        let h = Header::new(&chain, &e);

        assert_eq!(h.entry_hash(), e.hash());
        assert_eq!(h.link(), None);
        assert_ne!(h.hash(), "");
        assert!(h.validate());
    }

    #[test]
    /// tests for header.entry_type()
    fn entry_type() {
        let chain = test_chain();
        let t = "foo";
        let e = Entry::new(t, "");
        let h = Header::new(&chain, &e);

        assert_eq!(h.entry_type(), "foo");
    }

    #[test]
    /// tests for header.time()
    fn time() {
        let chain = test_chain();
        let t = "foo";
        let e = Entry::new(t, "");
        let h = Header::new(&chain, &e);

        assert_eq!(h.timestamp(), "");
    }

    #[test]
    /// tests for header.next()
    fn next() {
        let mut chain = test_chain();
        let t = "foo";

        // first header is genesis so next should be None
        let e1 = Entry::new(t, "");
        let p1 = chain
            .push_entry(&e1)
            .expect("pushing a valid entry to an exlusively owned chain shouldn't fail");
        let h1 = p1.header();

        assert_eq!(h1.link(), None);

        // second header next should be first header hash
        let e2 = Entry::new(t, "foo");
        let p2 = chain
            .push_entry(&e2)
            .expect("pushing a valid entry to an exlusively owned chain shouldn't fail");
        let h2 = p2.header();

        assert_eq!(h2.link(), Some(h1.hash()));
    }

    #[test]
    /// tests for header.entry()
    fn entry() {
        let chain = test_chain();
        let t = "foo";

        // header for an entry should contain the entry hash under entry()
        let e = Entry::new(t, "");
        let h = Header::new(&chain, &e);

        assert_eq!(h.entry_hash(), e.hash());
    }

    #[test]
    /// tests for header.type_next()
    fn type_next() {
        let mut chain = test_chain();
        let t1 = "foo";
        let t2 = "bar";

        // first header is genesis so next should be None
        let e1 = Entry::new(t1, "");
        let p1 = chain
            .push_entry(&e1)
            .expect("pushing a valid entry to an exlusively owned chain shouldn't fail");
        let h1 = p1.header();

        assert_eq!(h1.link_same_type(), None);

        // second header is a different type so next should be None
        let e2 = Entry::new(t2, "");
        let p2 = chain
            .push_entry(&e2)
            .expect("pushing a valid entry to an exlusively owned chain shouldn't fail");
        let h2 = p2.header();

        assert_eq!(h2.link_same_type(), None);

        // third header is same type as first header so next should be first header hash
        let e3 = Entry::new(t1, "");
        let p3 = chain
            .push_entry(&e3)
            .expect("pushing a valid entry to an exlusively owned chain shouldn't fail");
        let h3 = p3.header();

        assert_eq!(h3.link_same_type(), Some(h1.hash()));
    }

    #[test]
    /// tests for header.signature()
    fn signature() {
        let chain = test_chain();
        let t = "foo";

        let e = Entry::new(t, "");
        let h = Header::new(&chain, &e);

        assert_eq!("", h.entry_signature());
    }

    #[test]
    /// test header.hash() against a known value
    fn hash_known() {
        let chain = test_chain();
        let t = "foo";

        // check a known hash
        let e = Entry::new(t, "");
        let h = Header::new(&chain, &e);

        assert_eq!("QmSpmouzp7PoTFeEcrG1GWVGVneacJcuwU91wkDCGYvPZ9", h.hash());
    }

    #[test]
    /// test that different entry content returns different hashes
    fn hash_entry_content() {
        let chain = test_chain();
        let t = "fooType";

        // different entries must return different hashes
        let e1 = Entry::new(t, "");
        let h1 = Header::new(&chain, &e1);

        let e2 = Entry::new(t, "a");
        let h2 = Header::new(&chain, &e2);

        assert_ne!(h1.hash(), h2.hash());

        // same entry must return same hash
        let e3 = Entry::new(t, "");
        let h3 = Header::new(&chain, &e3);

        assert_eq!(h1.hash(), h3.hash());
    }

    #[test]
    /// test that different entry types returns different hashes
    fn hash_entry_type() {
        let chain = test_chain();
        let t1 = "foo";
        let t2 = "bar";
        let c = "baz";

        let e1 = Entry::new(t1, c);
        let e2 = Entry::new(t2, c);

        let h1 = Header::new(&chain, &e1);
        let h2 = Header::new(&chain, &e2);

        // different types must give different hashes
        assert_ne!(h1.hash(), h2.hash());
    }

    #[test]
    /// test that different chain state returns different hashes
    fn hash_chain_state() {
        // different chain, different hash
        let mut chain = test_chain();
        let t = "foo";
        let c = "bar";
        let e = Entry::new(t, c);
        let h = Header::new(&chain, &e);

        let p1 = chain
            .push_entry(&e)
            .expect("pushing a valid entry to an exlusively owned chain shouldn't fail");
        // p2 will have a different hash to p1 with the same entry as the chain state is different
        let p2 = chain
            .push_entry(&e)
            .expect("pushing a valid entry to an exlusively owned chain shouldn't fail");

        assert_eq!(h.hash(), p1.header().hash());
        assert_ne!(h.hash(), p2.header().hash());
    }

    #[test]
    /// test that different type_next returns different hashes
    fn hash_type_next() {
        // @TODO is it possible to test that type_next changes the hash in an isolated way?
        // @see https://github.com/holochain/holochain-rust/issues/76
    }

    #[test]
    /// tests for header.validate()
    fn validate() {
        let chain = test_chain();
        let t = "foo";

        let e = Entry::new(t, "");
        let h = Header::new(&chain, &e);

        assert!(h.validate());
    }

    #[test]
    /// tests for header.key()
    fn key() {
        assert_eq!(test_header().hash(), test_header().key());
    }
}
