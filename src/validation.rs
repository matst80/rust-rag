use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use jsonschema::JSONSchema;
use serde_json::Value;
use thiserror::Error;

use crate::db::{SchemaRecord, VectorStore};

/// Cache of compiled JSON Schema validators keyed by `type_name`. Compilation
/// is non-trivial; cache lookups gate every typed-entry store/update.
#[derive(Default)]
pub struct SchemaCache {
    inner: RwLock<HashMap<String, Arc<JSONSchema>>>,
}

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("unknown type: {0}")]
    UnknownType(String),
    #[error("schema for {type_name} is invalid: {message}")]
    InvalidSchema {
        type_name: String,
        message: String,
    },
    #[error("data failed schema validation for {type_name}")]
    Failed {
        type_name: String,
        errors: Vec<String>,
    },
    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}

impl SchemaCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn invalidate(&self, type_name: &str) {
        if let Ok(mut map) = self.inner.write() {
            map.remove(type_name);
        }
    }

    pub fn clear(&self) {
        if let Ok(mut map) = self.inner.write() {
            map.clear();
        }
    }

    fn compile(record: &SchemaRecord) -> Result<Arc<JSONSchema>, ValidationError> {
        let compiled = JSONSchema::compile(&record.json_schema).map_err(|e| {
            ValidationError::InvalidSchema {
                type_name: record.type_name.clone(),
                message: e.to_string(),
            }
        })?;
        Ok(Arc::new(compiled))
    }

    fn get_or_compile(
        &self,
        type_name: &str,
        store: &dyn VectorStore,
    ) -> Result<Arc<JSONSchema>, ValidationError> {
        {
            let map = self.inner.read().expect("schema cache poisoned");
            if let Some(v) = map.get(type_name) {
                return Ok(v.clone());
            }
        }
        let record = store
            .get_schema(type_name)
            .map_err(ValidationError::Storage)?
            .ok_or_else(|| ValidationError::UnknownType(type_name.to_string()))?;
        let compiled = Self::compile(&record)?;
        let mut map = self.inner.write().expect("schema cache poisoned");
        map.insert(type_name.to_string(), compiled.clone());
        Ok(compiled)
    }

    pub fn validate(
        &self,
        type_name: &str,
        data: &Value,
        store: &dyn VectorStore,
    ) -> Result<(), ValidationError> {
        let validator = self.get_or_compile(type_name, store)?;
        let result = validator.validate(data);
        match result {
            Ok(()) => Ok(()),
            Err(errs) => {
                let errors: Vec<String> = errs
                    .map(|e| format!("{}: {}", e.instance_path, e))
                    .collect();
                Err(ValidationError::Failed {
                    type_name: type_name.to_string(),
                    errors,
                })
            }
        }
    }
}

/// Verify a candidate JSON Schema is itself valid and compiles. Called on
/// schema registration before persisting.
pub fn validate_meta_schema(schema: &Value) -> Result<(), ValidationError> {
    JSONSchema::compile(schema).map_err(|e| ValidationError::InvalidSchema {
        type_name: String::new(),
        message: e.to_string(),
    })?;
    Ok(())
}

/// On-disk schema definition (one per file under `RAG_SCHEMA_DIR`).
#[derive(Debug, serde::Deserialize)]
struct BundledSchema {
    type_name: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    schema: Value,
}

/// Seed bundled schemas from disk into the store. Honors:
/// - `RAG_SCHEMA_DIR` (default `./assets/schemas`).
/// - `RAG_SCHEMA_RESEED=force` → overwrite existing rows; otherwise insert-if-missing.
pub fn seed_bundled_schemas(store: &dyn VectorStore) -> anyhow::Result<usize> {
    let dir = std::env::var("RAG_SCHEMA_DIR").unwrap_or_else(|_| "assets/schemas".to_string());
    let force = matches!(
        std::env::var("RAG_SCHEMA_RESEED").as_deref(),
        Ok("force") | Ok("1") | Ok("true")
    );
    let path = std::path::Path::new(&dir);
    if !path.is_dir() {
        return Ok(0);
    }
    let mut loaded = 0usize;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let file = entry.path();
        if file.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let raw = std::fs::read_to_string(&file)?;
        let bundled: BundledSchema = serde_json::from_str(&raw)
            .map_err(|e| anyhow::anyhow!("invalid schema file {file:?}: {e}"))?;
        if !force && store.get_schema(&bundled.type_name)?.is_some() {
            continue;
        }
        if let Err(e) = validate_meta_schema(&bundled.schema) {
            tracing::warn!(?e, "skipping invalid bundled schema {}", bundled.type_name);
            continue;
        }
        store.upsert_schema(SchemaRecord {
            type_name: bundled.type_name,
            json_schema: bundled.schema,
            title: bundled.title,
            description: bundled.description,
            created_at: 0,
            updated_at: 0,
        })?;
        loaded += 1;
    }
    Ok(loaded)
}
