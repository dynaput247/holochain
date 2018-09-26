use cas::{
    content::{Address, AddressableContent},
    storage::ContentAddressableStorage,
};
use error::HolochainError;
use std::{
    fs::{create_dir_all, read_to_string, write},
    path::{Path, MAIN_SEPARATOR},
};

pub struct FileContentAddressableStorage {
    path: String,
}

impl FileContentAddressableStorage {
    fn address_to_path(&self, address: &Address) -> String {
        format!("{}{}{}.json", self.path, MAIN_SEPARATOR, address)
    }
}

impl ContentAddressableStorage for FileContentAddressableStorage {
    fn add(&mut self, content: &AddressableContent) -> Result<(), HolochainError> {
        create_dir_all(&self.path)?;
        Ok(write(
            self.address_to_path(&content.address()),
            content.content(),
        )?)
    }

    fn contains(&self, address: &Address) -> Result<bool, HolochainError> {
        Ok(Path::new(&self.address_to_path(address)).is_file())
    }

    fn fetch<C: AddressableContent>(&self, address: &Address) -> Result<Option<C>, HolochainError> {
        if self.contains(&address)? {
            Ok(Some(C::from_content(&read_to_string(
                self.address_to_path(address),
            )?)))
        } else {
            Ok(None)
        }
    }
}
