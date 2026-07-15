use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use chrono::Utc;
use uuid::Uuid;

use crate::error::AppError;

use super::{migration::migrate_value, model::Blueprint, validation::validate_blueprint};

const BLUEPRINT_FILE: &str = "emanduite-project.json";

pub fn save_blueprint(path: &Path, blueprint: &Blueprint) -> Result<(), AppError> {
    validate_blueprint_path(path)?;
    if !validate_blueprint(blueprint).is_empty() {
        return Err(AppError::Validation);
    }
    let bytes = serde_json::to_vec_pretty(blueprint).map_err(|_| AppError::Internal)?;
    transactional_write(path, &bytes)
}

pub fn load_blueprint(path: &Path) -> Result<Blueprint, AppError> {
    validate_blueprint_path(path)?;
    recover_interrupted_save(path)?;
    if !path.exists() {
        return Err(AppError::NotFound);
    }
    let original = fs::read(path)?;
    let value = serde_json::from_slice(&original).map_err(|_| AppError::Validation)?;
    let outcome = migrate_value(value)?;
    let blueprint: Blueprint =
        serde_json::from_value(outcome.value).map_err(|_| AppError::Validation)?;
    if !validate_blueprint(&blueprint).is_empty() {
        return Err(AppError::Validation);
    }
    if outcome.changed {
        backup_original(path, outcome.from_version, &original)?;
        save_blueprint(path, &blueprint)?;
    }
    Ok(blueprint)
}

pub fn validate_blueprint_path(path: &Path) -> Result<(), AppError> {
    if !path.is_absolute() {
        return Err(AppError::InvalidPath);
    }
    if path.file_name().and_then(|v| v.to_str()) != Some(BLUEPRINT_FILE) {
        return Err(AppError::InvalidPath);
    }
    if path.components().any(|part| part.as_os_str() == "..") {
        return Err(AppError::InvalidPath);
    }
    Ok(())
}

fn transactional_write(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let parent = path.parent().ok_or(AppError::InvalidPath)?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".{BLUEPRINT_FILE}.{}.tmp", Uuid::new_v4()));
    let previous = parent.join(format!(".{BLUEPRINT_FILE}.previous"));
    let result = (|| -> Result<(), AppError> {
        let mut file = fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        if previous.exists() {
            fs::remove_file(&previous)?;
        }
        if path.exists() {
            fs::rename(path, &previous)?;
        }
        if let Err(error) = fs::rename(&temp, path) {
            if previous.exists() {
                let _ = fs::rename(&previous, path);
            }
            return Err(AppError::Io(error));
        }
        if previous.exists() {
            fs::remove_file(&previous)?;
        }
        if let Ok(dir) = fs::File::open(parent) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();
    if temp.exists() {
        let _ = fs::remove_file(temp);
    }
    result
}

fn recover_interrupted_save(path: &Path) -> Result<(), AppError> {
    let parent = path.parent().ok_or(AppError::InvalidPath)?;
    let previous = parent.join(format!(".{BLUEPRINT_FILE}.previous"));
    if !path.exists() && previous.exists() {
        fs::rename(previous, path)?;
    }
    Ok(())
}

fn backup_original(path: &Path, version: u32, bytes: &[u8]) -> Result<PathBuf, AppError> {
    let parent = path.parent().ok_or(AppError::InvalidPath)?;
    let backup_dir = parent.join(".backups");
    fs::create_dir_all(&backup_dir)?;
    let stamp = Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
    let backup = backup_dir.join(format!("blueprint-v{version}-{stamp}.json"));
    fs::write(&backup, bytes)?;
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_and_load_round_trip() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(BLUEPRINT_FILE);
        let blueprint = Blueprint::new_sqlite("Demo", "demo.db");
        save_blueprint(&path, &blueprint).unwrap();
        assert_eq!(load_blueprint(&path).unwrap(), blueprint);
    }

    #[test]
    fn migration_creates_backup() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(BLUEPRINT_FILE);
        fs::write(
            &path,
            r#"{"projectName":"Legacy","databasePath":"legacy.db"}"#,
        )
        .unwrap();
        let loaded = load_blueprint(&path).unwrap();
        assert_eq!(loaded.schema_version, 1);
        assert_eq!(
            fs::read_dir(directory.path().join(".backups"))
                .unwrap()
                .count(),
            1
        );
    }

    #[test]
    fn rejects_arbitrary_file_name() {
        assert!(matches!(
            validate_blueprint_path(Path::new("other.json")),
            Err(AppError::InvalidPath)
        ));
    }

    #[test]
    fn rejects_relative_blueprint_path() {
        assert!(matches!(
            validate_blueprint_path(Path::new(BLUEPRINT_FILE)),
            Err(AppError::InvalidPath)
        ));
    }
}
