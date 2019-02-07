//! EAV stands for entity-attribute-value. It is a pattern implemented here
//! for adding metadata about entries in the DHT, additionally
//! being used to define relationships between AddressableContent values.
//! See [wikipedia](https://en.wikipedia.org/wiki/Entity%E2%80%93attribute%E2%80%93value_model) to learn more about this pattern.

use crate::{
    cas::content::{Address, AddressableContent, Content},
    entry::{test_entry_a, test_entry_b, Entry},
    error::{HcResult, HolochainError},
    json::JsonString,
};
use chrono::offset::Utc;
use objekt;
use regex::Regex;
use std::{
    cmp::Ordering,
    collections::BTreeSet,
    convert::TryInto,
    sync::{Arc, RwLock},
};

use regex::RegexBuilder;
use std::{
    convert::TryFrom,
    fmt::{self, Debug},
};

/// Address of AddressableContent representing the EAV entity
pub type Entity = Address;

/// All Attribute values are pre-defined here. If ever a user-defined Attribute is needed,
/// just add a new Custom variant for it with a String parameter
#[derive(PartialEq, Eq, PartialOrd, Hash, Clone, Debug, Serialize, Deserialize, DefaultJson)]
#[serde(rename_all = "snake_case")]
pub enum Attribute {
    CrudStatus,
    CrudLink,
    EntryHeader,
    Link,
    LinkTag(String),
    PendingEntry,
}

impl fmt::Display for Attribute {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Attribute::CrudStatus => write!(f, "crud-status"),
            Attribute::CrudLink => write!(f, "crud-link"),
            Attribute::EntryHeader => write!(f, "entry-header"),
            Attribute::Link => write!(f, "link"),
            Attribute::LinkTag(name) => write!(f, "link__{}", name),
            Attribute::PendingEntry => write!(f, "pending-entry"),
        }
    }
}

lazy_static! {
    static ref LINK_REGEX: Regex =
        Regex::new(r"^link__(.*)$").expect("This string literal is a valid regex");
}

impl TryFrom<&str> for Attribute {
    type Error = HolochainError;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        use self::Attribute::*;
        if LINK_REGEX.is_match(s) {
            let tag = LINK_REGEX.captures(s)?.get(1)?.as_str().to_string();
            Ok(LinkTag(tag))
        } else {
            match s {
                "crud-status" => Ok(CrudStatus),
                "crud-link" => Ok(CrudLink),
                "entry-header" => Ok(EntryHeader),
                "link" => Ok(Link),
                "pending-entry" => Ok(PendingEntry),
                a => Err(HolochainError::ErrorGeneric(format!(
                    "Unknown attribute: {}",
                    a
                ))),
            }
        }
    }
}

impl TryFrom<String> for Attribute {
    type Error = HolochainError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.as_str().try_into()
    }
}

/// Address of AddressableContent representing the EAV value
pub type Value = Address;

// @TODO do we need this?
// unique (local to the source) monotonically increasing number that can be used for crdt/ordering
// @see https://papers.radixdlt.com/tempo/#logical-clocks
pub type Index = i64;

// @TODO do we need this?
// source agent asserting the meta
// type Source ...
/// The basic struct for EntityAttributeValue triple, implemented as AddressableContent
/// including the necessary serialization inherited.
#[derive(PartialEq, Eq, Hash, Clone, Debug, Serialize, Deserialize, DefaultJson)]
pub struct EntityAttributeValueIndex {
    entity: Entity,
    attribute: Attribute,
    value: Value,
    index: Index,
    // source: Source,
}

impl PartialOrd for EntityAttributeValueIndex {
    fn partial_cmp(&self, other: &EntityAttributeValueIndex) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EntityAttributeValueIndex {
    fn cmp(&self, other: &EntityAttributeValueIndex) -> Ordering {
        self.index.cmp(&other.index())
    }
}

#[derive(Clone, Debug)]
pub struct IndexQuery {
    start: Option<i64>,
    end: Option<i64>,
}

impl IndexQuery {
    pub fn start(&self) -> Option<i64> {
        self.start.clone()
    }

    pub fn end(&self) -> Option<i64> {
        self.end.clone()
    }

    pub fn new(start: i64, end: i64) -> IndexQuery {
        IndexQuery {
            start: Some(start),
            end: Some(end),
        }
    }
}

impl Default for IndexQuery {
    fn default() -> IndexQuery {
        IndexQuery {
            start: None,
            end: None,
        }
    }
}

impl AddressableContent for EntityAttributeValueIndex {
    fn content(&self) -> Content {
        self.to_owned().into()
    }

    fn try_from_content(content: &Content) -> Result<Self, HolochainError> {
        content.to_owned().try_into()
    }
}

fn validate_attribute(attribute: &Attribute) -> HcResult<()> {
    if let Attribute::LinkTag(name) = attribute {
        let regex = RegexBuilder::new(r#"[/:*?<>"'\\|+]"#)
            .build()
            .map_err(|_| HolochainError::ErrorGeneric("Could not create regex".to_string()))?;
        if !regex.is_match(name) {
            Ok(())
        } else {
            Err(HolochainError::ErrorGeneric(
                "Attribute name invalid".to_string(),
            ))
        }
    } else {
        Ok(())
    }
}

impl EntityAttributeValueIndex {
    pub fn new(
        entity: &Entity,
        attribute: &Attribute,
        value: &Value,
    ) -> HcResult<EntityAttributeValueIndex> {
        validate_attribute(attribute)?;
        Ok(EntityAttributeValueIndex {
            entity: entity.clone(),
            attribute: attribute.clone(),
            value: value.clone(),
            index: Utc::now().timestamp_nanos(),
        })
    }

    pub fn new_with_index(
        entity: &Entity,
        attribute: &Attribute,
        value: &Value,
        timestamp: i64,
    ) -> HcResult<EntityAttributeValueIndex> {
        validate_attribute(attribute)?;
        Ok(EntityAttributeValueIndex {
            entity: entity.clone(),
            attribute: attribute.clone(),
            value: value.clone(),
            index: timestamp,
        })
    }

    pub fn entity(&self) -> Entity {
        self.entity.clone()
    }

    pub fn attribute(&self) -> Attribute {
        self.attribute.clone()
    }

    pub fn value(&self) -> Value {
        self.value.clone()
    }

    pub fn index(&self) -> Index {
        self.index.clone()
    }

    pub fn set_index(&mut self, new_index: i64) {
        self.index = new_index
    }

    /// this is a predicate for matching on eav values. Useful for reducing duplicated filtered code.
    pub fn filter_on_eav<T>(eav: &T, e: Option<&T>) -> bool
    where
        T: PartialOrd,
    {
        e.map(|a| a == eav).unwrap_or(true)
    }
}

/// This provides a simple and flexible interface to define relationships between AddressableContent.
/// It does NOT provide storage for AddressableContent.
/// Use cas::storage::ContentAddressableStorage to store AddressableContent.
pub trait EntityAttributeValueStorage: objekt::Clone + Send + Sync + Debug {
    /// Adds the given EntityAttributeValue to the EntityAttributeValueStorage
    /// append only storage.
    fn add_eavi(
        &mut self,
        eav: &EntityAttributeValueIndex,
    ) -> Result<Option<EntityAttributeValueIndex>, HolochainError>;
    /// Fetch the set of EntityAttributeValues that match constraints according to the latest hash version
    /// - None = no constraint
    /// - Some(Entity) = requires the given entity (e.g. all a/v pairs for the entity)
    /// - Some(Attribute) = requires the given attribute (e.g. all links)
    /// - Some(Value) = requires the given value (e.g. all entities referencing an Address)
    fn fetch_eavi(
        &self,
        entity: Option<Entity>,
        attribute: Option<Attribute>,
        value: Option<Value>,
        index_query: IndexQuery,
    ) -> Result<BTreeSet<EntityAttributeValueIndex>, HolochainError>;
}

clone_trait_object!(EntityAttributeValueStorage);

#[derive(Clone, Debug)]
pub struct ExampleEntityAttributeValueStorage {
    storage: Arc<RwLock<BTreeSet<EntityAttributeValueIndex>>>,
}
impl ExampleEntityAttributeValueStorage {
    pub fn new() -> ExampleEntityAttributeValueStorage {
        ExampleEntityAttributeValueStorage {
            storage: Arc::new(RwLock::new(BTreeSet::new())),
        }
    }
}

pub fn increment_key_till_no_collision(
    mut eav: EntityAttributeValueIndex,
    map: BTreeSet<EntityAttributeValueIndex>,
) -> HcResult<EntityAttributeValueIndex> {
    if map
        .iter()
        .filter(|e| e.index == eav.index())
        .collect::<BTreeSet<&EntityAttributeValueIndex>>()
        .len()
        > 0
    {
        let timestamp = eav.clone().index + 1;
        eav.set_index(timestamp);
        increment_key_till_no_collision(eav, map)
    } else {
        Ok(eav)
    }
}

impl EntityAttributeValueStorage for ExampleEntityAttributeValueStorage {
    fn add_eavi(
        &mut self,
        eav: &EntityAttributeValueIndex,
    ) -> Result<Option<EntityAttributeValueIndex>, HolochainError> {
        let mut map = self.storage.write()?;
        let new_eav = increment_key_till_no_collision(eav.clone(), map.clone())?;
        map.insert(new_eav.clone());
        Ok(Some(new_eav.clone()))
    }

    fn fetch_eavi(
        &self,
        entity: Option<Entity>,
        attribute: Option<Attribute>,
        value: Option<Value>,
        index_query: IndexQuery,
    ) -> Result<BTreeSet<EntityAttributeValueIndex>, HolochainError> {
        let map = self.storage.read()?;
        let new_map = map.clone();
        let filtered = new_map
            .into_iter()
            .filter(|e| EntityAttributeValueIndex::filter_on_eav(&e.entity(), entity.as_ref()))
            .filter(|e| {
                EntityAttributeValueIndex::filter_on_eav(&e.attribute(), attribute.as_ref())
            })
            .filter(|e| EntityAttributeValueIndex::filter_on_eav(&e.value(), value.as_ref()))
            .filter(|e| {
                index_query
                    .start()
                    .map(|start| {
                        println!("start {:?}", e.index().clone());
                        println!("start condition {:?}", start.clone() <= e.index());
                        start <= e.index()
                    })
                    .unwrap_or_else(|| {
                        get_latest(e.clone(), map.clone())
                            .map(|latest| latest.index() == e.index())
                            .unwrap_or(false)
                    })
            })
            .filter(|e| {
                index_query
                    .end()
                    .map(|end| {
                        println!("end {:?}", e.index().clone());
                        println!("start condition {:?}", end.clone() >= e.index());
                        end >= e.index()
                    })
                    .unwrap_or_else(|| {
                        get_latest(e.clone(), map.clone())
                            .map(|latest| latest.index() == e.index())
                            .unwrap_or(false)
                    })
            })
            .collect::<BTreeSet<EntityAttributeValueIndex>>();

        Ok(filtered)
    }
}

pub fn get_latest(
    eav: EntityAttributeValueIndex,
    map: BTreeSet<EntityAttributeValueIndex>,
) -> HcResult<EntityAttributeValueIndex> {
    let filter = map
        .clone()
        .into_iter()
        .filter(|e| EntityAttributeValueIndex::filter_on_eav(&e.entity(), Some(&eav.entity())))
        .filter(|e| {
            EntityAttributeValueIndex::filter_on_eav(&e.attribute(), Some(&eav.attribute()))
        })
        .filter(|e| EntityAttributeValueIndex::filter_on_eav(&e.value(), Some(&eav.value())));
    filter.last().ok_or(HolochainError::ErrorGeneric(
        "Could not get last value".to_string(),
    ))
}

impl PartialEq for EntityAttributeValueStorage {
    fn eq(&self, other: &EntityAttributeValueStorage) -> bool {
        self.fetch_eavi(None, None, None, IndexQuery::default())
            == other.fetch_eavi(None, None, None, IndexQuery::default())
    }
}

pub fn test_eav_entity() -> Entry {
    test_entry_a()
}

pub fn test_eav_attribute() -> Attribute {
    Attribute::LinkTag("foo-attribute".into())
}

pub fn test_eav_value() -> Entry {
    test_entry_b()
}

pub fn test_eav() -> EntityAttributeValueIndex {
    EntityAttributeValueIndex::new_with_index(
        &test_eav_entity().address(),
        &test_eav_attribute(),
        &test_eav_value().address(),
        0,
    )
    .expect("Could not create eav")
}

pub fn test_eav_content() -> Content {
    test_eav().content()
}

pub fn test_eav_address() -> Address {
    test_eav().address()
}

pub fn eav_round_trip_test_runner(
    entity_content: impl AddressableContent + Clone,
    attribute: Attribute,
    value_content: impl AddressableContent + Clone,
) {
    let eav = EntityAttributeValueIndex::new(
        &entity_content.address(),
        &attribute,
        &value_content.address(),
    )
    .expect("Could not create EAV");
    let mut eav_storage = ExampleEntityAttributeValueStorage::new();

    assert_eq!(
        BTreeSet::new(),
        eav_storage
            .fetch_eavi(
                Some(entity_content.address()),
                Some(attribute.clone()),
                Some(value_content.address()),
                IndexQuery::default()
            )
            .expect("could not fetch eav"),
    );

    eav_storage.add_eavi(&eav).expect("could not add eav");

    let mut expected = BTreeSet::new();
    expected.insert(eav.clone());
    // some examples of constraints that should all return the eav
    for (e, a, v) in vec![
        // constrain all
        (
            Some(entity_content.address()),
            Some(attribute.clone()),
            Some(value_content.address()),
        ),
        // open entity
        (None, Some(attribute.clone()), Some(value_content.address())),
        // open attribute
        (
            Some(entity_content.address()),
            None,
            Some(value_content.address()),
        ),
        // open value
        (
            Some(entity_content.address()),
            Some(attribute.clone()),
            None,
        ),
        // open
        (None, None, None),
    ] {
        assert_eq!(
            expected,
            eav_storage
                .fetch_eavi(e, a, v, IndexQuery::default())
                .expect("could not fetch eav")
        );
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::{
        cas::{
            content::{AddressableContent, AddressableContentTestSuite, ExampleAddressableContent},
            storage::{
                test_content_addressable_storage, EavTestSuite, ExampleContentAddressableStorage,
            },
        },
        eav::EntityAttributeValueIndex,
        json::RawString,
    };

    pub fn test_eav_storage() -> ExampleEntityAttributeValueStorage {
        ExampleEntityAttributeValueStorage::new()
    }

    #[test]
    fn example_eav_round_trip() {
        let eav_storage = test_eav_storage();
        let entity =
            ExampleAddressableContent::try_from_content(&JsonString::from(RawString::from("foo")))
                .unwrap();
        let attribute = "favourite-color".to_string();
        let value =
            ExampleAddressableContent::try_from_content(&JsonString::from(RawString::from("blue")))
                .unwrap();

        EavTestSuite::test_round_trip(eav_storage, entity, attribute, value)
    }

    #[test]
    fn example_eav_one_to_many() {
        EavTestSuite::test_one_to_many::<
            ExampleAddressableContent,
            ExampleEntityAttributeValueStorage,
        >(test_eav_storage());
    }

    #[test]
    fn example_eav_many_to_one() {
        EavTestSuite::test_many_to_one::<
            ExampleAddressableContent,
            ExampleEntityAttributeValueStorage,
        >(test_eav_storage());
    }

    #[test]
    fn example_eav_range() {
        EavTestSuite::test_range::<ExampleAddressableContent, ExampleEntityAttributeValueStorage>(
            test_eav_storage(),
        );
    }

    #[test]
    /// show AddressableContent implementation
    fn addressable_content_test() {
        // from_content()
        AddressableContentTestSuite::addressable_content_trait_test::<EntityAttributeValueIndex>(
            test_eav_content(),
            test_eav(),
            test_eav_address(),
        );
    }

    #[test]
    /// show CAS round trip
    fn cas_round_trip_test() {
        let addressable_contents = vec![test_eav()];
        AddressableContentTestSuite::addressable_content_round_trip::<
            EntityAttributeValueIndex,
            ExampleContentAddressableStorage,
        >(addressable_contents, test_content_addressable_storage());
    }

    #[test]
    fn attribute_try_from_string() {
        assert_eq!("crud-status".try_into(), Ok(Attribute::CrudStatus));
        assert_eq!(
            "link__tagalog".try_into(),
            Ok(Attribute::LinkTag("tagalog".into()))
        );
        assert!(
            (r"unknown \\and// invalid / attribute".try_into() as Result<Attribute, _>).is_err(),
        );
    }

    #[test]
    fn validate_attribute_paths() {
        assert!(EntityAttributeValueIndex::new(
            &test_eav_entity().address(),
            &Attribute::LinkTag("abc".into()),
            &test_eav_entity().address()
        )
        .is_ok());
        assert!(EntityAttributeValueIndex::new(
            &test_eav_entity().address(),
            &Attribute::LinkTag("abc123".into()),
            &test_eav_entity().address()
        )
        .is_ok());
        assert!(EntityAttributeValueIndex::new(
            &test_eav_entity().address(),
            &Attribute::LinkTag("123".into()),
            &test_eav_entity().address()
        )
        .is_ok());
        assert!(EntityAttributeValueIndex::new(
            &test_eav_entity().address(),
            &Attribute::LinkTag("link_:{}".into()),
            &test_eav_entity().address()
        )
        .is_err());
        assert!(EntityAttributeValueIndex::new(
            &test_eav_entity().address(),
            &Attribute::LinkTag("link_\"".into()),
            &test_eav_entity().address()
        )
        .is_err());
        assert!(EntityAttributeValueIndex::new(
            &test_eav_entity().address(),
            &Attribute::LinkTag("link_/".into()),
            &test_eav_entity().address()
        )
        .is_err());
        assert!(EntityAttributeValueIndex::new(
            &test_eav_entity().address(),
            &Attribute::LinkTag("link_\\".into()),
            &test_eav_entity().address()
        )
        .is_err());
        assert!(EntityAttributeValueIndex::new(
            &test_eav_entity().address(),
            &Attribute::LinkTag("link_?".into()),
            &test_eav_entity().address()
        )
        .is_err());
    }

}
