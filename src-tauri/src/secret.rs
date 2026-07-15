use std::{collections::HashMap, sync::Mutex};

use uuid::Uuid;

use crate::error::AppError;

const SERVICE: &str = "emanduite";

pub trait SecretStore: Send + Sync {
    fn put(&self, project_id: &str, key: &str, value: &str) -> Result<String, AppError>;
    fn contains(&self, secret_ref: &str) -> Result<bool, AppError>;
    fn get(&self, secret_ref: &str) -> Result<String, AppError>;
    fn delete(&self, secret_ref: &str) -> Result<(), AppError>;
}

pub struct KeyringSecretStore;

impl KeyringSecretStore {
    fn entry(secret_ref: &str) -> Result<keyring::Entry, AppError> {
        let (project_id, key) = parse_secret_ref(secret_ref)?;
        keyring::Entry::new(SERVICE, &format!("{project_id}/{key}"))
            .map_err(|_| AppError::SecretStore)
    }
}

impl SecretStore for KeyringSecretStore {
    fn put(&self, project_id: &str, key: &str, value: &str) -> Result<String, AppError> {
        validate_parts(project_id, key)?;
        if value.is_empty() {
            return Err(AppError::Validation);
        }
        let secret_ref = format!("keyring://{SERVICE}/{project_id}/{key}");
        Self::entry(&secret_ref)?
            .set_password(value)
            .map_err(|_| AppError::SecretStore)?;
        Ok(secret_ref)
    }

    fn contains(&self, secret_ref: &str) -> Result<bool, AppError> {
        match Self::entry(secret_ref)?.get_password() {
            Ok(_) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(_) => Err(AppError::SecretStore),
        }
    }

    fn get(&self, secret_ref: &str) -> Result<String, AppError> {
        Self::entry(secret_ref)?
            .get_password()
            .map_err(|_| AppError::SecretStore)
    }

    fn delete(&self, secret_ref: &str) -> Result<(), AppError> {
        match Self::entry(secret_ref)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(AppError::SecretStore),
        }
    }
}

#[derive(Default)]
pub struct InMemorySecretStore {
    values: Mutex<HashMap<String, String>>,
}

impl SecretStore for InMemorySecretStore {
    fn put(&self, project_id: &str, key: &str, value: &str) -> Result<String, AppError> {
        validate_parts(project_id, key)?;
        let secret_ref = format!("keyring://{SERVICE}/{project_id}/{key}");
        self.values
            .lock()
            .map_err(|_| AppError::Internal)?
            .insert(secret_ref.clone(), value.into());
        Ok(secret_ref)
    }
    fn contains(&self, secret_ref: &str) -> Result<bool, AppError> {
        parse_secret_ref(secret_ref)?;
        Ok(self
            .values
            .lock()
            .map_err(|_| AppError::Internal)?
            .contains_key(secret_ref))
    }
    fn get(&self, secret_ref: &str) -> Result<String, AppError> {
        parse_secret_ref(secret_ref)?;
        self.values
            .lock()
            .map_err(|_| AppError::Internal)?
            .get(secret_ref)
            .cloned()
            .ok_or(AppError::NotFound)
    }
    fn delete(&self, secret_ref: &str) -> Result<(), AppError> {
        parse_secret_ref(secret_ref)?;
        self.values
            .lock()
            .map_err(|_| AppError::Internal)?
            .remove(secret_ref);
        Ok(())
    }
}

fn validate_parts(project_id: &str, key: &str) -> Result<(), AppError> {
    if Uuid::parse_str(project_id).is_err()
        || key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err(AppError::Validation);
    }
    Ok(())
}

fn parse_secret_ref(secret_ref: &str) -> Result<(&str, &str), AppError> {
    let rest = secret_ref
        .strip_prefix("keyring://emanduite/")
        .ok_or(AppError::Validation)?;
    let (project_id, key) = rest.split_once('/').ok_or(AppError::Validation)?;
    validate_parts(project_id, key)?;
    Ok((project_id, key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_uses_opaque_reference() {
        let store = InMemorySecretStore::default();
        let project = Uuid::new_v4().to_string();
        let secret_ref = store
            .put(&project, "database.password", "super-secret")
            .unwrap();
        assert!(!secret_ref.contains("super-secret"));
        assert!(store.contains(&secret_ref).unwrap());
        assert_eq!(store.get(&secret_ref).unwrap(), "super-secret");
        store.delete(&secret_ref).unwrap();
        assert!(!store.contains(&secret_ref).unwrap());
    }

    #[test]
    fn rejects_unsafe_key() {
        let store = InMemorySecretStore::default();
        assert!(store
            .put(&Uuid::new_v4().to_string(), "../password", "x")
            .is_err());
    }
}
