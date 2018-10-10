use actor::{AskSelf, Protocol, SYS};
use cas::content::{Address, AddressableContent, Content};
use entry::{test_entry_unique, Entry};
use error::HolochainError;
use riker::actors::*;
use snowflake;
use std::{collections::HashMap, fmt::Debug, sync::mpsc::channel, thread};

/// content addressable store (CAS)
/// implements storage in memory or persistently
/// anything implementing AddressableContent can be added and fetched by address
/// CAS is append only
pub trait ContentAddressableStorage: Clone + Send + Sync {
    /// adds AddressableContent to the ContentAddressableStorage by its Address as Content
    fn add(&mut self, content: &AddressableContent) -> Result<(), HolochainError>;
    /// true if the Address is in the Store, false otherwise.
    /// may be more efficient than retrieve depending on the implementation.
    fn contains(&self, address: &Address) -> Result<bool, HolochainError>;
    /// returns Some AddressableContent if it is in the Store, else None
    /// AddressableContent::from_content() can be used to allow the compiler to infer the type
    /// @see the fetch implementation for ExampleCas in the cas module tests
    fn fetch<C: AddressableContent>(&self, address: &Address) -> Result<Option<C>, HolochainError>;
}

#[derive(Clone)]
/// some struct to show an example ContentAddressableStorage implementation
/// there is no persistence or concurrency in this example so use a raw HashMap
/// @see ExampleContentAddressableStorageActor
pub struct ExampleContentAddressableStorage {
    actor: ActorRef<Protocol>,
}

impl ExampleContentAddressableStorage {
    pub fn new() -> Result<ExampleContentAddressableStorage, HolochainError> {
        Ok(ExampleContentAddressableStorage {
            actor: ExampleContentAddressableStorageActor::new_ref()?,
        })
    }
}

pub fn test_content_addressable_storage() -> ExampleContentAddressableStorage {
    ExampleContentAddressableStorage::new().expect("could not build example cas")
}

impl ContentAddressableStorage for ExampleContentAddressableStorage {
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

/// show an example Actor for ContentAddressableStorage
/// a key requirement of the CAS is that cloning doesn't undermine data consistency
/// a key requirement of the CAS is that multithreading doesn't undermine data consistency
/// actors deliver on both points through the ActorRef<Protocol> abstraction
/// cloned actor references point to the same actor with the same internal state
/// actors have internal message queues to co-ordinate requests
/// the tradeoff is boilerplate + some overhead from the actor system
pub struct ExampleContentAddressableStorageActor {
    storage: HashMap<Address, Content>,
}

impl ExampleContentAddressableStorageActor {
    pub fn new() -> ExampleContentAddressableStorageActor {
        ExampleContentAddressableStorageActor {
            storage: HashMap::new(),
        }
    }

    fn actor() -> BoxActor<Protocol> {
        Box::new(ExampleContentAddressableStorageActor::new())
    }

    fn props() -> BoxActorProd<Protocol> {
        Props::new(Box::new(ExampleContentAddressableStorageActor::actor))
    }

    pub fn new_ref() -> Result<ActorRef<Protocol>, HolochainError> {
        Ok(SYS.actor_of(
            ExampleContentAddressableStorageActor::props(),
            // all actors have the same ID to allow round trip across clones
            &snowflake::ProcessUniqueId::new().to_string(),
        )?)
    }

    fn unthreadable_add(
        &mut self,
        address: &Address,
        content: &Content,
    ) -> Result<(), HolochainError> {
        self.storage.insert(address.clone(), content.clone());
        Ok(())
    }

    fn unthreadable_contains(&self, address: &Address) -> Result<bool, HolochainError> {
        Ok(self.storage.contains_key(address))
    }

    fn unthreadable_fetch(&self, address: &Address) -> Result<Option<Content>, HolochainError> {
        Ok(self.storage.get(address).cloned())
    }
}

/// this is all boilerplate
impl Actor for ExampleContentAddressableStorageActor {
    type Msg = Protocol;

    fn receive(
        &mut self,
        context: &Context<Self::Msg>,
        message: Self::Msg,
        sender: Option<ActorRef<Self::Msg>>,
    ) {
        sender
            .try_tell(
                match message {
                    Protocol::CasAdd(address, content) => {
                        Protocol::CasAddResult(self.unthreadable_add(&address, &content))
                    }
                    Protocol::CasContains(address) => {
                        Protocol::CasContainsResult(self.unthreadable_contains(&address))
                    }
                    Protocol::CasFetch(address) => {
                        Protocol::CasFetchResult(self.unthreadable_fetch(&address))
                    }
                    _ => unreachable!(),
                },
                Some(context.myself()),
            )
            .expect("failed to tell MemoryStorageActor sender");
    }
}

//A struct for our test suite that infers a type of ContentAddressableStorage
pub struct StorageTestSuite<T>
where
    T: ContentAddressableStorage,
{
    pub cas: T,
    /// it is important that every cloned copy of any CAS has a consistent view to data
    pub cas_clone: T,
}

impl<T> StorageTestSuite<T>
where
    T: ContentAddressableStorage + 'static,
{
    pub fn new(cas: T) -> StorageTestSuite<T> {
        StorageTestSuite {
            cas_clone: cas.clone(),
            cas: cas,
        }
    }

    //does round trip test that can infer two Addressable Content Types
    pub fn round_trip_test<Addressable, OtherAddressable>(
        mut self,
        content: Content,
        other_content: Content,
    ) where
        Addressable: AddressableContent + Clone + PartialEq + Debug,
        OtherAddressable: AddressableContent + Clone + PartialEq + Debug,
    {
        // based on associate type we call the right from_content function
        let addressable_content = Addressable::from_content(&content);
        let other_addressable_content = OtherAddressable::from_content(&other_content);

        // do things that would definitely break if cloning would show inconsistent data
        let both_cas = vec![self.cas.clone(), self.cas_clone.clone()];

        for cas in both_cas.iter() {
            assert_eq!(Ok(false), cas.contains(&addressable_content.address()));
            assert_eq!(
                Ok(None),
                cas.fetch::<Addressable>(&addressable_content.address())
            );
            assert_eq!(
                Ok(false),
                cas.contains(&other_addressable_content.address())
            );
            assert_eq!(
                Ok(None),
                cas.fetch::<OtherAddressable>(&other_addressable_content.address())
            );
        }

        // round trip some AddressableContent through the ContentAddressableStorage
        assert_eq!(Ok(()), self.cas.add(&content));

        for cas in both_cas.iter() {
            assert_eq!(Ok(true), cas.contains(&content.address()));
            assert_eq!(Ok(false), cas.contains(&other_content.address()));
            assert_eq!(Ok(Some(content.clone())), cas.fetch(&content.address()));
        }

        // multiple types of AddressableContent can sit in a single ContentAddressableStorage
        // the safety of this is only as good as the hashing algorithm(s) used
        assert_eq!(Ok(()), self.cas_clone.add(&other_content));

        for cas in both_cas.iter() {
            assert_eq!(Ok(true), cas.contains(&content.address()));
            assert_eq!(Ok(true), cas.contains(&other_content.address()));
            assert_eq!(Ok(Some(content.clone())), cas.fetch(&content.address()));
            assert_eq!(
                Ok(Some(other_content.clone())),
                cas.fetch(&other_content.address())
            );
        }

        // show consistent view on data across threads

        let entry = test_entry_unique();

        // initially should not find entry
        let thread_cas = self.cas.clone();
        let thread_entry = entry.clone();
        let (tx1, rx1) = channel();
        thread::spawn(move || {
            assert_eq!(
                None,
                thread_cas
                    .fetch::<Entry>(&thread_entry.address())
                    .expect("could not fetch from cas")
            );
            tx1.send(true).unwrap();
        });

        // should be able to add an entry found in the next channel
        let mut thread_cas = self.cas.clone();
        let thread_entry = entry.clone();
        let (tx2, rx2) = channel();
        thread::spawn(move || {
            rx1.recv().unwrap();
            thread_cas
                .add(&thread_entry)
                .expect("could not add entry to cas");
            tx2.send(true).expect("could not kick off next thread");
        });

        let thread_cas = self.cas.clone();
        let thread_entry = entry.clone();
        let handle = thread::spawn(move || {
            rx2.recv().unwrap();
            assert_eq!(
                Some(thread_entry.clone()),
                thread_cas
                    .fetch(&thread_entry.address())
                    .expect("could not fetch from cas")
            )
        });

        handle.join().unwrap();
    }
}

#[cfg(test)]
pub mod tests {
    use cas::{
        content::{ExampleAddressableContent, OtherExampleAddressableContent},
        storage::{test_content_addressable_storage, StorageTestSuite},
    };

    /// show that content of different types can round trip through the same storage
    #[test]
    fn example_content_round_trip_test() {
        let test_suite = StorageTestSuite::new(test_content_addressable_storage());
        test_suite.round_trip_test::<ExampleAddressableContent, OtherExampleAddressableContent>(
            String::from("foo"),
            String::from("bar"),
        );
    }
}
