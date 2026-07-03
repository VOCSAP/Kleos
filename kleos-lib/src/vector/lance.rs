use super::{VectorHit, VectorIndex};
use crate::{EngError, Result};
use arrow_array::types::Float32Type;
use arrow_array::{Array, FixedSizeListArray, Float32Array, Int64Array, RecordBatch};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use async_trait::async_trait;
use futures::TryStreamExt;
use lancedb::index::vector::IvfHnswPqIndexBuilder;
use lancedb::index::Index;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::DistanceType;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::warn;

// Table/index constants live in the parent module so consumers (admin routes,
// config reporting) can reference them without the ml feature; re-exported
// here to keep existing `vector::lance::` paths working when ml is on.
pub use super::{CHUNK_TABLE_NAME, DEFAULT_TABLE_NAME, LANCE_SCHEMA_VERSION, MIN_ROWS_FOR_INDEX};

/// Name of the embedding column in every Lance table.
const VECTOR_COLUMN: &str = "vector";

// Wrap a LanceDB error with a short context string.
fn lance_err(context: &str, err: impl std::fmt::Display) -> EngError {
    EngError::Internal(format!("{}: {}", context, err))
}

// Arrow schema for the vector table: (memory_id: Int64, vector: FixedSizeList<Float32>).
fn vector_schema(dimensions: usize) -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("memory_id", DataType::Int64, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                dimensions as i32,
            ),
            false,
        ),
    ]))
}

// Build a single-row RecordBatch for one memory's embedding.
fn single_embedding_batch(
    memory_id: i64,
    embedding: &[f32],
    schema: SchemaRef,
    dimensions: usize,
) -> Result<RecordBatch> {
    if embedding.len() != dimensions {
        return Err(EngError::InvalidInput(format!(
            "embedding dimension mismatch: expected {}, got {}",
            dimensions,
            embedding.len()
        )));
    }

    let vectors = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
        [Some(
            embedding.iter().copied().map(Some).collect::<Vec<_>>(),
        )],
        dimensions as i32,
    );

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(Int64Array::from(vec![memory_id])),
            Arc::new(vectors),
        ],
    )
    .map_err(|e| lance_err("build LanceDB record batch", e))
}

// LanceDB-backed VectorIndex implementation: one table per index, lazily
// created, with an optional IVF_HNSW_PQ index once the row count is large enough.
pub struct LanceIndex {
    db: lancedb::Connection,
    table: RwLock<Option<lancedb::Table>>,
    table_name: String,
    dimensions: usize,
}

// Construction and internal table/index maintenance helpers.
impl LanceIndex {
    /// Open (or create) the default vector table at `path`.
    pub async fn open(path: impl AsRef<str>, dimensions: usize) -> Result<Self> {
        Self::open_with_table(path, dimensions, DEFAULT_TABLE_NAME).await
    }

    /// Open (or create) a named vector table at `path`.
    pub async fn open_with_table(
        path: impl AsRef<str>,
        dimensions: usize,
        table_name: &str,
    ) -> Result<Self> {
        let db = lancedb::connect(path.as_ref())
            .execute()
            .await
            .map_err(|e| lance_err("connect LanceDB", e))?;

        let index = Self {
            db,
            table: RwLock::new(None),
            table_name: table_name.to_string(),
            dimensions,
        };
        let _ = index.ensure_table().await?;
        Ok(index)
    }

    /// Get the cached table handle, or open/create it (and repair a stale
    /// pre-Phase-5.21 schema) on first use.
    async fn ensure_table(&self) -> Result<lancedb::Table> {
        if let Some(table) = self.table.read().await.as_ref().cloned() {
            return Ok(table);
        }

        let mut table_guard = self.table.write().await;
        if let Some(table) = table_guard.as_ref().cloned() {
            return Ok(table);
        }

        let table_names = self
            .db
            .table_names()
            .execute()
            .await
            .map_err(|e| lance_err("list LanceDB tables", e))?;

        let table = if table_names.iter().any(|name| name == &self.table_name) {
            let existing = self
                .db
                .open_table(&self.table_name)
                .execute()
                .await
                .map_err(|e| lance_err("open LanceDB vector table", e))?;

            // Phase 5.21 schema mismatch detection: if the on-disk schema still
            // contains a "user_id" field (created before Phase 5.21), wipe and
            // recreate the table with the new (memory_id, vector) schema.
            // The table will be repopulated lazily by subsequent insert() calls
            // or in bulk by build_lance_index_from_existing on the next dreamer
            // / ingestion sweep.
            let persisted_schema = existing
                .schema()
                .await
                .map_err(|e| lance_err("read LanceDB table schema", e))?;
            // Validate vector dimensions match runtime config.
            if let Ok(field) = persisted_schema.field_with_name(VECTOR_COLUMN) {
                if let DataType::FixedSizeList(_, on_disk_dims) = field.data_type() {
                    let on_disk = *on_disk_dims as usize;
                    if on_disk != self.dimensions {
                        return Err(EngError::InvalidInput(format!(
                            "LanceDB table '{}' has {}-dim vectors but runtime expects {}-dim. \
                             Re-embed with the correct model or delete the lance directory to rebuild.",
                            self.table_name, on_disk, self.dimensions
                        )));
                    }
                }
            }

            let has_stale_user_id = persisted_schema.field_with_name("user_id").is_ok();
            if has_stale_user_id {
                warn!(
                    "LanceDB table '{}' has stale user_id column (pre-Phase-5.21 schema). \
                     Dropping and recreating with new schema. \
                     The index will be repopulated by the next ingestion/dreamer sweep.",
                    self.table_name
                );
                self.db
                    .drop_table(&self.table_name, &[])
                    .await
                    .map_err(|e| lance_err("drop stale LanceDB vector table", e))?;
                self.db
                    .create_empty_table(&self.table_name, vector_schema(self.dimensions))
                    .execute()
                    .await
                    .map_err(|e| lance_err("recreate LanceDB vector table after schema wipe", e))?
            } else {
                existing
            }
        } else {
            self.db
                .create_empty_table(&self.table_name, vector_schema(self.dimensions))
                .execute()
                .await
                .map_err(|e| lance_err("create LanceDB vector table", e))?
        };

        *table_guard = Some(table.clone());
        Ok(table)
    }

    /// Build the vector index once the table has enough rows, if it doesn't exist yet.
    async fn ensure_vector_index(&self, table: &lancedb::Table) {
        let row_count = table.count_rows(None).await.unwrap_or(0);
        if row_count < MIN_ROWS_FOR_INDEX {
            return;
        }

        if self.vector_index_exists(table).await {
            return;
        }

        if let Err(e) = self.create_hnsw_index(table).await {
            warn!("LanceDB vector index create skipped: {}", e);
        }
    }

    /// Whether an index already exists on the vector column.
    async fn vector_index_exists(&self, table: &lancedb::Table) -> bool {
        table
            .list_indices()
            .await
            .map(|indices| {
                indices
                    .iter()
                    .any(|index| index.columns == vec![VECTOR_COLUMN.to_string()])
            })
            .unwrap_or(false)
    }

    /// Create an IVF_HNSW_PQ index on the vector column using cosine distance.
    async fn create_hnsw_index(&self, table: &lancedb::Table) -> Result<()> {
        let builder = IvfHnswPqIndexBuilder::default().distance_type(DistanceType::Cosine);
        table
            .create_index(&[VECTOR_COLUMN], Index::IvfHnswPq(builder))
            .execute()
            .await
            .map_err(|e| lance_err("create LanceDB IVF_HNSW_PQ index", e))
    }

    /// Drop any existing index on the vector column.
    async fn drop_vector_index(&self, table: &lancedb::Table) -> Result<()> {
        let indices = table
            .list_indices()
            .await
            .map_err(|e| lance_err("list LanceDB indices", e))?;
        for index in indices {
            if index.columns == vec![VECTOR_COLUMN.to_string()] {
                table
                    .drop_index(&index.name)
                    .await
                    .map_err(|e| lance_err("drop LanceDB vector index", e))?;
            }
        }
        Ok(())
    }
}

// VectorIndex impl backed by a LanceDB table.
#[async_trait]
impl VectorIndex for LanceIndex {
    /// Upsert a single memory's embedding (delete any existing row, then add).
    async fn insert(&self, memory_id: i64, embedding: &[f32]) -> Result<()> {
        let table = self.ensure_table().await?;
        let schema = vector_schema(self.dimensions);
        let batch = single_embedding_batch(memory_id, embedding, schema.clone(), self.dimensions)?;

        table
            .delete(&format!("memory_id = {}", memory_id))
            .await
            .map_err(|e| lance_err("delete previous LanceDB vector row", e))?;
        table
            .add(vec![batch])
            .execute()
            .await
            .map_err(|e| lance_err("insert LanceDB vector row", e))?;

        self.ensure_vector_index(&table).await;
        Ok(())
    }

    /// Batched upsert. One delete (`memory_id IN (...)`) + one add per call
    /// amortises Lance's per-row manifest rewrite across the whole batch,
    /// which dominates cost during /admin/backfill_chunks once the table is
    /// non-trivially populated. The single-row `insert` path stays in place
    /// for normal store traffic where there is nothing to batch.
    async fn insert_many(&self, items: &[(i64, Vec<f32>)]) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }
        for (_, emb) in items {
            if emb.len() != self.dimensions {
                return Err(EngError::InvalidInput(format!(
                    "embedding dimension mismatch: expected {}, got {}",
                    self.dimensions,
                    emb.len()
                )));
            }
        }

        let table = self.ensure_table().await?;
        let schema = vector_schema(self.dimensions);

        let ids: Vec<i64> = items.iter().map(|(id, _)| *id).collect();
        let embeddings: Vec<Option<Vec<Option<f32>>>> = items
            .iter()
            .map(|(_, e)| Some(e.iter().copied().map(Some).collect()))
            .collect();
        let vectors = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
            embeddings,
            self.dimensions as i32,
        );
        let batch = RecordBatch::try_new(
            schema,
            vec![Arc::new(Int64Array::from(ids.clone())), Arc::new(vectors)],
        )
        .map_err(|e| lance_err("build LanceDB batch record batch", e))?;

        let id_list = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
        table
            .delete(&format!("memory_id IN ({})", id_list))
            .await
            .map_err(|e| lance_err("delete previous LanceDB vector rows (batch)", e))?;
        table
            .add(vec![batch])
            .execute()
            .await
            .map_err(|e| lance_err("insert LanceDB vector rows (batch)", e))?;

        self.ensure_vector_index(&table).await;
        Ok(())
    }

    /// Run a nearest-neighbour search and return hits ranked by distance.
    async fn search(&self, embedding: &[f32], limit: usize) -> Result<Vec<VectorHit>> {
        if embedding.len() != self.dimensions {
            return Err(EngError::InvalidInput(format!(
                "embedding dimension mismatch: expected {}, got {}",
                self.dimensions,
                embedding.len()
            )));
        }

        let table = self.ensure_table().await?;
        let batches: Vec<RecordBatch> = table
            .query()
            .nearest_to(embedding)
            .map_err(|e| lance_err("create LanceDB vector query", e))?
            .limit(limit)
            .execute()
            .await
            .map_err(|e| lance_err("execute LanceDB vector query", e))?
            .try_collect()
            .await
            .map_err(|e| lance_err("collect LanceDB vector results", e))?;

        let mut hits = Vec::new();
        for batch in batches {
            let memory_idx = batch
                .schema()
                .index_of("memory_id")
                .map_err(|e| lance_err("read LanceDB memory_id column", e))?;
            let memory_ids = batch
                .column(memory_idx)
                .as_any()
                .downcast_ref::<Int64Array>()
                .ok_or_else(|| {
                    EngError::Internal("LanceDB memory_id column is not Int64".into())
                })?;

            let distance_values = batch
                .schema()
                .index_of("_distance")
                .ok()
                .and_then(|idx| batch.column(idx).as_any().downcast_ref::<Float32Array>());

            for row in 0..batch.num_rows() {
                let distance = distance_values.and_then(|dist| {
                    if dist.is_null(row) {
                        None
                    } else {
                        Some(dist.value(row))
                    }
                });
                hits.push(VectorHit {
                    memory_id: memory_ids.value(row),
                    distance,
                    rank: hits.len(),
                });
            }
        }

        Ok(hits)
    }

    /// Delete a memory's vector row by id.
    async fn delete(&self, memory_id: i64) -> Result<()> {
        let table = self.ensure_table().await?;
        table
            .delete(&format!("memory_id = {}", memory_id))
            .await
            .map_err(|e| lance_err("delete LanceDB vector row", e))?;
        Ok(())
    }

    /// Count rows currently stored in the vector table.
    async fn count(&self) -> Result<usize> {
        let table = self.ensure_table().await?;
        table
            .count_rows(None)
            .await
            .map_err(|e| lance_err("count LanceDB vector rows", e))
    }

    /// Rebuild the vector index if the table has enough rows; `replace` forces
    /// a rebuild even if an index already exists.
    async fn rebuild_index(&self, replace: bool) -> Result<bool> {
        let table = self.ensure_table().await?;
        let row_count = table
            .count_rows(None)
            .await
            .map_err(|e| lance_err("count LanceDB vector rows", e))?;
        if row_count < MIN_ROWS_FOR_INDEX {
            return Ok(false);
        }

        let exists = self.vector_index_exists(&table).await;
        if exists && !replace {
            return Ok(false);
        }
        if exists {
            self.drop_vector_index(&table).await?;
        }

        self.create_hnsw_index(&table).await?;
        Ok(true)
    }
}

// Tests for LanceIndex insert/search/rebuild behaviour.
#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    // Unique scratch directory for a LanceDB table under the OS temp dir.
    fn temp_path() -> String {
        let dir = std::env::temp_dir().join(format!("kleos-lance-{}", Uuid::new_v4()));
        dir.to_string_lossy().to_string()
    }

    // A table below MIN_ROWS_FOR_INDEX must not build an IVF_HNSW_PQ index.
    #[tokio::test]
    async fn rebuild_index_skipped_below_row_threshold() {
        let path = temp_path();
        let index = LanceIndex::open(&path, 4).await.expect("open lance");
        index
            .insert(1, &[0.1, 0.2, 0.3, 0.4])
            .await
            .expect("insert");
        index
            .insert(2, &[0.9, 0.8, 0.7, 0.6])
            .await
            .expect("insert");
        assert_eq!(index.count().await.expect("count"), 2);

        let rebuilt = index.rebuild_index(true).await.expect("rebuild");
        assert!(
            !rebuilt,
            "small table must skip IVF_HNSW_PQ build (row_count < {})",
            MIN_ROWS_FOR_INDEX
        );

        let _ = std::fs::remove_dir_all(&path);
    }

    // Search without a built index (linear scan) must still return the nearest row.
    #[tokio::test]
    async fn insert_then_search_linear_scan_returns_nearest() {
        let path = temp_path();
        let index = LanceIndex::open(&path, 4).await.expect("open lance");
        index
            .insert(42, &[1.0, 0.0, 0.0, 0.0])
            .await
            .expect("insert");
        index
            .insert(43, &[0.0, 1.0, 0.0, 0.0])
            .await
            .expect("insert");

        let hits = index
            .search(&[1.0, 0.0, 0.0, 0.0], 1)
            .await
            .expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].memory_id, 42);

        let _ = std::fs::remove_dir_all(&path);
    }

    // rebuild_index on an empty table must be a no-op, not an error.
    #[tokio::test]
    async fn rebuild_index_noop_when_table_empty() {
        let path = temp_path();
        let index = LanceIndex::open(&path, 4).await.expect("open lance");
        assert_eq!(index.count().await.expect("count"), 0);
        let rebuilt = index.rebuild_index(false).await.expect("rebuild");
        assert!(!rebuilt);

        let _ = std::fs::remove_dir_all(&path);
    }

    // Phase 5.21: the schema must only contain memory_id and vector, not user_id.
    #[test]
    fn lance_schema_v2_excludes_user_id() {
        let schema = vector_schema(1024);
        let field_names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
        assert_eq!(
            field_names,
            vec!["memory_id", "vector"],
            "Phase 5.21: schema must contain only memory_id and vector, not user_id"
        );
    }
}
