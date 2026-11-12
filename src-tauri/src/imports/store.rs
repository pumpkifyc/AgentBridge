use super::ImportRecord;
use crate::db::ImportDatabase;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("本地导入记录操作失败：{0}")]
    Database(#[from] crate::db::DatabaseError),
}

pub trait ImportStore {
    fn upsert(&self, record: &ImportRecord) -> Result<(), StoreError>;
    fn list(&self) -> Result<Vec<ImportRecord>, StoreError>;
    fn get(&self, id: &str) -> Result<Option<ImportRecord>, StoreError>;
    fn mark_retried(&self, id: &str, at: i64) -> Result<(), StoreError>;
    fn delete(&self, id: &str) -> Result<(), StoreError>;
    fn continuation_prompt(&self, thread_id: &str) -> Result<Option<String>, StoreError>;
}

impl ImportStore for ImportDatabase {
    fn upsert(&self, record: &ImportRecord) -> Result<(), StoreError> {
        ImportDatabase::upsert(self, record).map_err(StoreError::Database)
    }

    fn list(&self) -> Result<Vec<ImportRecord>, StoreError> {
        ImportDatabase::list(self).map_err(StoreError::Database)
    }

    fn get(&self, id: &str) -> Result<Option<ImportRecord>, StoreError> {
        ImportDatabase::get(self, id).map_err(StoreError::Database)
    }

    fn mark_retried(&self, id: &str, at: i64) -> Result<(), StoreError> {
        ImportDatabase::mark_retried(self, id, at).map_err(StoreError::Database)
    }

    fn delete(&self, id: &str) -> Result<(), StoreError> {
        ImportDatabase::delete(self, id).map_err(StoreError::Database)
    }

    fn continuation_prompt(&self, thread_id: &str) -> Result<Option<String>, StoreError> {
        ImportDatabase::find_by_thread(self, thread_id).map_err(StoreError::Database)
    }
}
