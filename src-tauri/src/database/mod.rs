pub mod sqlite;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{
    blueprint::{DatabaseConfig, Table},
    error::AppError,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    pub provider: String,
    pub database_label: String,
    pub sqlite_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IntrospectionResult {
    pub tables: Vec<Table>,
    pub diagnostics: Vec<DatabaseDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseDiagnostic {
    pub code: String,
    pub object: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaOperation {
    pub kind: String,
    pub object_id: String,
    pub destructive: bool,
    pub sql_preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MigrationPlan {
    pub operations: Vec<SchemaOperation>,
    pub requires_backup: bool,
}

#[async_trait]
pub trait DatabaseAdapter: Send + Sync {
    async fn test_connection(&self, config: &DatabaseConfig) -> Result<ConnectionStatus, AppError>;
    async fn introspect(&self, config: &DatabaseConfig) -> Result<IntrospectionResult, AppError>;
    async fn plan_schema_changes(
        &self,
        _config: &DatabaseConfig,
        _operations: &[SchemaOperation],
    ) -> Result<MigrationPlan, AppError> {
        Err(AppError::UnsupportedVersion)
    }
    async fn apply_schema_changes(
        &self,
        _config: &DatabaseConfig,
        _plan: &MigrationPlan,
    ) -> Result<(), AppError> {
        Err(AppError::UnsupportedVersion)
    }
}
