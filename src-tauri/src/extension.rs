use std::{
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionDocument {
    pub path: String,
    pub language: String,
    pub content: String,
    pub valid: bool,
    pub diagnostics: Vec<String>,
}

pub fn load_extension(
    project_file: &Path,
    relative_path: &str,
    language: &str,
) -> Result<ExtensionDocument, AppError> {
    let path = resolve_extension_path(project_file, relative_path, false)?;
    let content = if path.exists() {
        fs::read_to_string(&path)?
    } else {
        String::new()
    };
    Ok(document(relative_path, language, content))
}

pub fn validate_extension(
    relative_path: &str,
    language: &str,
    content: String,
) -> Result<ExtensionDocument, AppError> {
    validate_relative_path(relative_path)?;
    validate_language(language)?;
    Ok(document(relative_path, language, content))
}

pub fn save_extension(
    project_file: &Path,
    relative_path: &str,
    language: &str,
    content: String,
    format: bool,
) -> Result<ExtensionDocument, AppError> {
    if content.len() > 1_048_576 || content.contains('\0') {
        return Err(AppError::Validation);
    }
    let content = if format {
        format_content(language, &content)?
    } else {
        content
    };
    let result = document(relative_path, language, content);
    if !result.valid {
        return Ok(result);
    }
    let path = resolve_extension_path(project_file, relative_path, true)?;
    let parent = path.parent().ok_or(AppError::InvalidPath)?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".extension.{}.tmp", Uuid::new_v4()));
    let mut file = fs::File::create(&temp)?;
    file.write_all(result.content.as_bytes())?;
    file.sync_all()?;
    let backup = parent.join(format!(".extension.{}.bak", Uuid::new_v4()));
    let had_previous = path.exists();
    if had_previous {
        fs::rename(&path, &backup)?;
    }
    if let Err(error) = fs::rename(&temp, &path) {
        if had_previous {
            let _ = fs::rename(&backup, &path);
        }
        let _ = fs::remove_file(&temp);
        return Err(error.into());
    }
    if had_previous {
        fs::remove_file(backup)?;
    }
    Ok(result)
}

fn document(relative_path: &str, language: &str, content: String) -> ExtensionDocument {
    let diagnostics = content_diagnostics(language, &content);
    ExtensionDocument {
        path: relative_path.into(),
        language: language.into(),
        valid: diagnostics.is_empty(),
        diagnostics,
        content,
    }
}

fn format_content(language: &str, content: &str) -> Result<String, AppError> {
    validate_language(language)?;
    if language == "json" {
        let value: serde_json::Value =
            serde_json::from_str(content).map_err(|_| AppError::Validation)?;
        let mut output = serde_json::to_string_pretty(&value).map_err(|_| AppError::Internal)?;
        output.push('\n');
        return Ok(output);
    }
    Ok(format!("{}\n", content.replace("\r\n", "\n").trim_end()))
}

fn content_diagnostics(language: &str, content: &str) -> Vec<String> {
    if validate_language(language).is_err() {
        return vec!["Unsupported extension language".into()];
    }
    if content.len() > 1_048_576 {
        return vec!["Extension file exceeds the 1 MiB editor limit".into()];
    }
    if content.contains('\0') {
        return vec!["Extension file contains a null byte".into()];
    }
    if language == "json" {
        return serde_json::from_str::<serde_json::Value>(content)
            .err()
            .map(|error| {
                vec![format!(
                    "Invalid JSON at line {} column {}",
                    error.line(),
                    error.column()
                )]
            })
            .unwrap_or_default();
    }
    let mut stack = Vec::new();
    for character in content.chars() {
        match character {
            '{' | '(' | '[' => stack.push(character),
            '}' if stack.pop() != Some('{') => return vec!["Unbalanced closing brace".into()],
            ')' if stack.pop() != Some('(') => return vec!["Unbalanced closing parenthesis".into()],
            ']' if stack.pop() != Some('[') => return vec!["Unbalanced closing bracket".into()],
            _ => {}
        }
    }
    if stack.is_empty() {
        Vec::new()
    } else {
        vec!["Unclosed delimiter".into()]
    }
}

fn resolve_extension_path(
    project_file: &Path,
    relative_path: &str,
    allow_missing: bool,
) -> Result<PathBuf, AppError> {
    validate_relative_path(relative_path)?;
    let project = project_file
        .canonicalize()
        .map_err(|_| AppError::InvalidPath)?;
    if project.file_name().and_then(|value| value.to_str()) != Some("emanduite-project.json") {
        return Err(AppError::InvalidPath);
    }
    let root = project
        .parent()
        .ok_or(AppError::InvalidPath)?
        .join("extensions");
    fs::create_dir_all(&root)?;
    let root = root.canonicalize().map_err(|_| AppError::InvalidPath)?;
    let target = root.join(relative_path);
    if target.exists() {
        let canonical = target.canonicalize().map_err(|_| AppError::InvalidPath)?;
        if !canonical.starts_with(&root) || !canonical.is_file() {
            return Err(AppError::InvalidPath);
        }
        Ok(canonical)
    } else if allow_missing || !target.exists() {
        if !target.starts_with(&root) {
            return Err(AppError::InvalidPath);
        }
        Ok(target)
    } else {
        Err(AppError::NotFound)
    }
}

fn validate_relative_path(value: &str) -> Result<(), AppError> {
    let path = Path::new(value);
    if value.trim().is_empty()
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

fn validate_language(language: &str) -> Result<(), AppError> {
    if matches!(language, "json" | "typescript" | "javascript" | "css") {
        Ok(())
    } else {
        Err(AppError::Validation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_extension_traversal() {
        assert!(matches!(
            validate_relative_path("../secret.txt"),
            Err(AppError::InvalidPath)
        ));
    }

    #[test]
    fn validates_and_formats_json() {
        let result =
            validate_extension("config.json", "json", "{\"enabled\":true}".into()).unwrap();
        assert!(result.valid);
        assert!(format_content("json", &result.content)
            .unwrap()
            .contains("  \"enabled\""));
    }

    #[test]
    fn invalid_typescript_is_not_saved() {
        let directory = tempfile::tempdir().unwrap();
        let project = directory.path().join("emanduite-project.json");
        fs::write(&project, "{}").unwrap();
        let result = save_extension(
            &project,
            "hooks/example.ts",
            "typescript",
            "export const value = {".into(),
            false,
        )
        .unwrap();
        assert!(!result.valid);
        assert!(!directory
            .path()
            .join("extensions/hooks/example.ts")
            .exists());
    }
}
