use super::types::TeamError;

const TEAM_CREDENTIAL_SERVICE: &str = "io.nexusops.client.team";

pub trait CredentialStore: Send + Sync {
    fn get(&self, reference: &str) -> Result<Option<String>, TeamError>;
    fn set(&self, reference: &str, secret: &str) -> Result<(), TeamError>;
    fn delete(&self, reference: &str) -> Result<(), TeamError>;
}

#[derive(Default)]
pub struct OsCredentialStore;

impl OsCredentialStore {
    fn entry(reference: &str) -> Result<keyring::Entry, TeamError> {
        if reference.len() != 64 || !reference.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(TeamError::CredentialStore);
        }
        keyring::Entry::new(TEAM_CREDENTIAL_SERVICE, &format!("connection:{reference}"))
            .map_err(|_| TeamError::CredentialStore)
    }
}

impl CredentialStore for OsCredentialStore {
    fn get(&self, reference: &str) -> Result<Option<String>, TeamError> {
        match Self::entry(reference)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(TeamError::CredentialStore),
        }
    }

    fn set(&self, reference: &str, secret: &str) -> Result<(), TeamError> {
        Self::entry(reference)?
            .set_password(secret)
            .map_err(|_| TeamError::CredentialStore)
    }

    fn delete(&self, reference: &str) -> Result<(), TeamError> {
        match Self::entry(reference)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(TeamError::CredentialStore),
        }
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;
    use std::{collections::HashMap, sync::Mutex};

    #[derive(Default)]
    pub struct MemoryCredentialStore(pub Mutex<HashMap<String, String>>);

    impl CredentialStore for MemoryCredentialStore {
        fn get(&self, reference: &str) -> Result<Option<String>, TeamError> {
            Ok(self.0.lock().unwrap().get(reference).cloned())
        }
        fn set(&self, reference: &str, secret: &str) -> Result<(), TeamError> {
            self.0
                .lock()
                .unwrap()
                .insert(reference.into(), secret.into());
            Ok(())
        }
        fn delete(&self, reference: &str) -> Result<(), TeamError> {
            self.0.lock().unwrap().remove(reference);
            Ok(())
        }
    }
}

#[cfg(all(test, target_os = "windows"))]
mod windows_tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    #[ignore = "writes and immediately removes one synthetic Windows credential"]
    fn os_credential_round_trip_leaves_no_entry() {
        let nonce = uuid::Uuid::new_v4().to_string();
        let reference = format!("{:x}", Sha256::digest(nonce.as_bytes()));
        let secret = format!("nx_synthetic_keyring_{nonce}");
        let store = OsCredentialStore;

        let written = store.set(&reference, &secret);
        let loaded = written
            .as_ref()
            .ok()
            .and_then(|_| store.get(&reference).ok());
        let deleted = store.delete(&reference);
        let missing = store.get(&reference);

        written.expect("write synthetic credential");
        assert_eq!(loaded.flatten().as_deref(), Some(secret.as_str()));
        deleted.expect("delete synthetic credential");
        assert_eq!(missing.expect("read deleted credential"), None);
    }
}
