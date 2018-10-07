use cas::{
    content::{Address, AddressableContent},
    storage::ContentAddressableStorage,
};
use error::HolochainError;
mod actor;
use actor::{AskSelf, Protocol};
use cas::memory::actor::MemoryStorageActor;
use riker::actors::*;

#[derive(Clone, Debug, PartialEq)]
pub struct MemoryStorage {
    actor: ActorRef<Protocol>,
}

impl MemoryStorage {
    pub fn new() -> Result<MemoryStorage, HolochainError> {
        Ok(MemoryStorage {
            actor: MemoryStorageActor::new_ref()?,
        })
    }
}

impl ContentAddressableStorage for MemoryStorage {
    fn add(&mut self, content: &AddressableContent) -> Result<(), HolochainError> {
        let response = self
            .actor
            .block_on_ask(Protocol::CasAdd(content.address(), content.content()))?;
        unwrap_to!(response => Protocol::CasAddResult).clone()
    }

    fn contains(&self, address: &Address) -> Result<bool, HolochainError> {
        let response = self
            .actor
            .block_on_ask(Protocol::CasContains(address.clone()))?;
        unwrap_to!(response => Protocol::CasContainsResult).clone()
    }

    fn fetch<AC: AddressableContent>(
        &self,
        address: &Address,
    ) -> Result<Option<AC>, HolochainError> {
        let response = self
            .actor
            .block_on_ask(Protocol::CasFetch(address.clone()))?;
        let content = unwrap_to!(response => Protocol::CasFetchResult).clone()?;
        Ok(match content {
            Some(c) => Some(AC::from_content(&c)),
            None => None,
        })
    }
}

#[cfg(test)]
pub mod tests {
    use cas::{
        content::tests::{ExampleAddressableContent, OtherExampleAddressableContent},
        memory::MemoryStorage,
        storage::tests::StorageTestSuite,
    };

    pub fn test_memory_storage() -> MemoryStorage {
        MemoryStorage::new().expect("could not create memory storage")
    }

    #[test]
    fn memory_round_trip() {
        let test_suite = StorageTestSuite::new(test_memory_storage());
        test_suite.round_trip_test::<ExampleAddressableContent, OtherExampleAddressableContent>(
            String::from("foo"),
            String::from("bar"),
        );
    }

}
