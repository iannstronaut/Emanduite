pub mod sqlite;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{
    blueprint::{Column, DatabaseConfig, ForeignKey, Table},
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SchemaOperation {
    AddTable {
        operation_id: String,
        table: Table,
    },
    DropTable {
        operation_id: String,
        table_name: String,
    },
    AddColumn {
        operation_id: String,
        table_name: String,
        column: Column,
    },
    DropColumn {
        operation_id: String,
        table_name: String,
        column_name: String,
    },
    RenameColumn {
        operation_id: String,
        table_name: String,
        from: String,
        to: String,
    },
    AddForeignKey {
        operation_id: String,
        table_name: String,
        foreign_key: ForeignKey,
    },
    DropForeignKey {
        operation_id: String,
        table_name: String,
        foreign_key_id: String,
    },
}

impl SchemaOperation {
    pub fn operation_id(&self) -> &str {
        match self {
            Self::AddTable { operation_id, .. }
            | Self::DropTable { operation_id, .. }
            | Self::AddColumn { operation_id, .. }
            | Self::DropColumn { operation_id, .. }
            | Self::RenameColumn { operation_id, .. }
            | Self::AddForeignKey { operation_id, .. }
            | Self::DropForeignKey { operation_id, .. } => operation_id,
        }
    }

    pub fn destructive(&self) -> bool {
        matches!(
            self,
            Self::DropTable { .. } | Self::DropColumn { .. } | Self::DropForeignKey { .. }
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MigrationPlan {
    pub id: String,
    pub schema_fingerprint: String,
    pub operations: Vec<SchemaOperation>,
    pub statements: Vec<String>,
    pub sql_preview: String,
    pub destructive: bool,
    pub requires_backup: bool,
    pub confirmation_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub plan_id: String,
    pub backup_path: String,
    pub statements_applied: usize,
}

#[async_trait]
pub trait DatabaseAdapter: Send + Sync {
    async fn test_connection(&self, config: &DatabaseConfig) -> Result<ConnectionStatus, AppError>;
    async fn introspect(&self, config: &DatabaseConfig) -> Result<IntrospectionResult, AppError>;
    async fn plan_schema_changes(
        &self,
        config: &DatabaseConfig,
        operations: &[SchemaOperation],
    ) -> Result<MigrationPlan, AppError>;
    async fn apply_schema_changes(
        &self,
        config: &DatabaseConfig,
        plan: &MigrationPlan,
        confirmation_token: Option<&str>,
    ) -> Result<ApplyResult, AppError>;
}
