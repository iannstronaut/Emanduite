use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::AppError;

use super::{GeneratedFile, GenerationConflict, GenerationManifest, ManifestFile, Ownership};

pub(super) fn apply_plan(
    target: &Path,
    files: &[GeneratedFile],
    template_id: &str,
    template_version: &str,
    blueprint_hash: &str,
) -> Result<(GenerationManifest, Vec<GenerationConflict>, usize, usize), AppError> {
    let previous = load_manifest(target)?;
    let mut conflicts = Vec::new();
    let mut written = 0;
    let mut preserved = 0;
    let mut next_files = BTreeMap::new();
    let planned: BTreeSet<_> = files.iter().map(|file| file.path.as_str()).collect();

    for file in files {
        validate_relative_path(&file.path)?;
        let destination = target.join(&file.path);
        match file.owner {
            Ownership::Generated => {
                let desired_hash = hash_bytes(file.content.as_bytes());
                let prior = previous
                    .as_ref()
                    .and_then(|value| value.files.get(&file.path));
                let current_hash = read_hash(&destination)?;
                let safe_to_write = match (&current_hash, prior) {
                    (None, _) => true,
                    (Some(current), Some(entry)) if entry.owner == Ownership::Generated => {
                        current == &entry.hash
                    }
                    (Some(current), None) => current == &desired_hash,
                    _ => false,
                };
                if safe_to_write {
                    if current_hash.as_deref() != Some(desired_hash.as_str()) {
                        atomic_write(&destination, file.content.as_bytes())?;
                        written += 1;
                    } else {
                        preserved += 1;
                    }
                    next_files.insert(
                        file.path.clone(),
                        ManifestFile {
                            owner: Ownership::Generated,
                            hash: desired_hash,
                        },
                    );
                } else {
                    let artifact = conflict_artifact_path(target, &file.path, "generated")?;
                    atomic_write(&artifact, file.content.as_bytes())?;
                    conflicts.push(GenerationConflict {
                        path: file.path.clone(),
                        artifact_path: relative_string(target, &artifact)?,
                        reason: "generated file was modified outside Emanduite".into(),
                    });
                    if let Some(entry) = prior {
                        next_files.insert(file.path.clone(), entry.clone());
                    }
                    preserved += 1;
                }
            }
            Ownership::User => {
                if destination.exists() {
                    preserved += 1;
                } else if file.content.is_empty() {
                    continue;
                } else {
                    atomic_write(&destination, file.content.as_bytes())?;
                    written += 1;
                }
                let hash = read_hash(&destination)?.ok_or(AppError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "user-owned file was not created",
                )))?;
                next_files.insert(
                    file.path.clone(),
                    ManifestFile {
                        owner: Ownership::User,
                        hash,
                    },
                );
            }
        }
    }

    if let Some(previous) = &previous {
        for (path, entry) in &previous.files {
            if planned.contains(path.as_str()) {
                continue;
            }
            if entry.owner == Ownership::User {
                next_files.insert(path.clone(), entry.clone());
                continue;
            }
            validate_relative_path(path)?;
            let destination = target.join(path);
            let Some(current_hash) = read_hash(&destination)? else {
                continue;
            };
            if current_hash == entry.hash {
                fs::remove_file(destination)?;
            } else {
                let artifact = conflict_artifact_path(target, path, "delete.json")?;
                let notice = serde_json::to_vec_pretty(&StaleConflict {
                    path,
                    reason:
                        "generated file is no longer in the template but contains manual changes",
                })
                .map_err(|_| AppError::Internal)?;
                atomic_write(&artifact, &notice)?;
                conflicts.push(GenerationConflict {
                    path: path.clone(),
                    artifact_path: relative_string(target, &artifact)?,
                    reason: "stale generated file contains manual changes".into(),
                });
                next_files.insert(path.clone(), entry.clone());
            }
        }
    }

    let manifest = GenerationManifest {
        format_version: 1,
        template_id: template_id.into(),
        template_version: template_version.into(),
        blueprint_hash: blueprint_hash.into(),
        files: next_files,
    };
    let mut bytes = serde_json::to_vec_pretty(&manifest).map_err(|_| AppError::Internal)?;
    bytes.push(b'\n');
    atomic_write(&target.join(".emanduite/manifest.json"), &bytes)?;
    Ok((manifest, conflicts, written, preserved))
}

#[derive(Serialize)]
struct StaleConflict<'a> {
    path: &'a str,
    reason: &'static str,
}

fn load_manifest(target: &Path) -> Result<Option<GenerationManifest>, AppError> {
    let path = target.join(".emanduite/manifest.json");
    if !path.exists() {
        return Ok(None);
    }
    let manifest: GenerationManifest =
        serde_json::from_slice(&fs::read(path)?).map_err(|_| AppError::Validation)?;
    if manifest.format_version != 1 {
        return Err(AppError::UnsupportedVersion);
    }
    for path in manifest.files.keys() {
        validate_relative_path(path)?;
    }
    Ok(Some(manifest))
}

fn conflict_artifact_path(
    target: &Path,
    relative: &str,
    suffix: &str,
) -> Result<PathBuf, AppError> {
    validate_relative_path(relative)?;
    Ok(target
        .join(".emanduite/conflicts")
        .join(format!("{relative}.{suffix}")))
}

fn relative_string(root: &Path, path: &Path) -> Result<String, AppError> {
    Ok(path
        .strip_prefix(root)
        .map_err(|_| AppError::InvalidPath)?
        .to_string_lossy()
        .replace('\\', "/"))
}

pub(super) fn validate_relative_path(value: &str) -> Result<(), AppError> {
    let path = Path::new(value);
    if value.is_empty()
        || value.contains('\0')
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(AppError::InvalidPath);
    }
    Ok(())
}

fn read_hash(path: &Path) -> Result<Option<String>, AppError> {
    if !path.exists() {
        return Ok(None);
    }
    if !path.is_file() {
        return Err(AppError::InvalidPath);
    }
    Ok(Some(hash_bytes(&fs::read(path)?)))
}

pub(super) fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let parent = path.parent().ok_or(AppError::InvalidPath)?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".emanduite.{}.tmp", Uuid::new_v4()));
    let mut file = fs::File::create(&temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    let previous = parent.join(format!(".emanduite.{}.previous", Uuid::new_v4()));
    let had_previous = path.exists();
    if had_previous {
        if !path.is_file() {
            let _ = fs::remove_file(&temp);
            return Err(AppError::InvalidPath);
        }
        fs::rename(path, &previous)?;
    }
    if let Err(error) = fs::rename(&temp, path) {
        if had_previous {
            let _ = fs::rename(&previous, path);
        }
        let _ = fs::remove_file(temp);
        return Err(error.into());
    }
    if had_previous {
        let _ = fs::remove_file(previous);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generated(path: &str, content: &str) -> GeneratedFile {
        GeneratedFile {
            path: path.into(),
            owner: Ownership::Generated,
            content: content.into(),
        }
    }

    #[test]
    fn repeat_generation_is_idempotent_and_manual_change_conflicts() {
        let directory = tempfile::tempdir().unwrap();
        let files = vec![generated("src/a.ts", "export const a = 1;\n")];
        let (first, conflicts, written, _) =
            apply_plan(directory.path(), &files, "template", "1.0.0", "blueprint").unwrap();
        assert!(conflicts.is_empty());
        assert_eq!(written, 1);
        let (second, conflicts, written, _) =
            apply_plan(directory.path(), &files, "template", "1.0.0", "blueprint").unwrap();
        assert_eq!(first, second);
        assert!(conflicts.is_empty());
        assert_eq!(written, 0);

        fs::write(directory.path().join("src/a.ts"), "// user change\n").unwrap();
        let (_, conflicts, _, _) = apply_plan(
            directory.path(),
            &[generated("src/a.ts", "export const a = 2;\n")],
            "template",
            "1.0.0",
            "blueprint",
        )
        .unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(
            fs::read_to_string(directory.path().join("src/a.ts")).unwrap(),
            "// user change\n"
        );
        assert!(directory.path().join(&conflicts[0].artifact_path).is_file());
    }

    #[test]
    fn user_owned_file_is_never_rewritten() {
        let directory = tempfile::tempdir().unwrap();
        let initial = GeneratedFile {
            path: "src/extensions/hook.ts".into(),
            owner: Ownership::User,
            content: "first\n".into(),
        };
        apply_plan(directory.path(), &[initial], "template", "1.0.0", "one").unwrap();
        fs::write(directory.path().join("src/extensions/hook.ts"), "custom\n").unwrap();
        let next = GeneratedFile {
            path: "src/extensions/hook.ts".into(),
            owner: Ownership::User,
            content: "second\n".into(),
        };
        apply_plan(directory.path(), &[next], "template", "1.0.0", "two").unwrap();
        assert_eq!(
            fs::read_to_string(directory.path().join("src/extensions/hook.ts")).unwrap(),
            "custom\n"
        );
    }
}
