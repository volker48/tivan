//! The shell half of persistence: what to do with the browser's storage at load and at save.
//! [`persist`](crate::persist) owns the format; this owns the policy for what a failure costs, so
//! it runs against an in-memory [`Storage`] under `cargo test`.

use std::fmt;

use crate::domain::{Doc, seed_doc};
use crate::notice::Notice;
use crate::persist::{BACKUP_KEY, DecodeError, KEY, LoadPlan, encode, plan_load};

/// A key-value store of strings, which is all `localStorage` is.
pub trait Storage {
    fn get(&self, key: &str) -> Result<Option<String>, StorageError>;
    fn set(&self, key: &str, value: &str) -> Result<(), StorageError>;
}

/// Names the operation and the underlying cause, so a banner can say what failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageError(String);

impl StorageError {
    pub fn new(message: impl Into<String>) -> StorageError {
        StorageError(message.into())
    }
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StorageError {}

/// The document to start with, what to tell the teacher, and whether this session may write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loaded {
    pub doc: Doc,
    pub notice: Option<Notice>,
    /// False when the stored data must survive this session untouched. The save effect checks
    /// this rather than the notice, because the notice can be replaced.
    pub can_save: bool,
}

/// Reads storage once, at startup. An unreadable blob is copied to [`BACKUP_KEY`] before the
/// first save can replace it, and saving stays off when that copy fails. A blob from a newer build is never replaced, and its copy is not
/// made because the original stays where it is.
pub fn load(storage: &impl Storage) -> Loaded {
    let stored = match storage.get(KEY) {
        Ok(stored) => stored,
        Err(error) => {
            return Loaded {
                doc: seed_doc(),
                notice: Some(Notice::StorageUnreadable(error.to_string())),
                can_save: false,
            };
        }
    };
    match plan_load(stored.as_deref()) {
        LoadPlan::FirstRun => Loaded {
            doc: seed_doc(),
            notice: None,
            can_save: true,
        },
        LoadPlan::Use(doc) => Loaded {
            doc,
            notice: None,
            can_save: true,
        },
        LoadPlan::StartFresh {
            reason: DecodeError::NewerVersion(_),
            ..
        } => Loaded {
            doc: seed_doc(),
            notice: Some(Notice::OutOfDate),
            can_save: false,
        },
        LoadPlan::StartFresh { stash, .. } => match storage.set(BACKUP_KEY, &stash) {
            Ok(()) => Loaded {
                doc: seed_doc(),
                notice: Some(Notice::StartedFresh),
                can_save: true,
            },
            Err(error) => Loaded {
                doc: seed_doc(),
                notice: Some(Notice::BackupFailed(error.to_string())),
                can_save: false,
            },
        },
    }
}

pub fn save(storage: &impl Storage, doc: &Doc) -> Result<(), StorageError> {
    storage.set(KEY, &encode(doc))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;
    use crate::persist::SCHEMA_VERSION;

    #[derive(Default)]
    struct MemoryStorage {
        entries: RefCell<HashMap<String, String>>,
        writes: RefCell<Vec<String>>,
        fail_reads: bool,
        fail_writes_to: Option<&'static str>,
    }

    impl MemoryStorage {
        fn with(key: &str, value: &str) -> MemoryStorage {
            let storage = MemoryStorage::default();
            storage
                .entries
                .borrow_mut()
                .insert(key.to_owned(), value.to_owned());
            storage
        }

        fn entry(&self, key: &str) -> Option<String> {
            self.entries.borrow().get(key).cloned()
        }
    }

    impl Storage for MemoryStorage {
        fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
            if self.fail_reads {
                return Err(StorageError::new("read blocked"));
            }
            Ok(self.entry(key))
        }

        fn set(&self, key: &str, value: &str) -> Result<(), StorageError> {
            if self.fail_writes_to == Some(key) {
                return Err(StorageError::new("quota exceeded"));
            }
            self.writes.borrow_mut().push(key.to_owned());
            self.entries
                .borrow_mut()
                .insert(key.to_owned(), value.to_owned());
            Ok(())
        }
    }

    fn newer_blob() -> String {
        format!(r#"{{"version":{},"doc":{{}}}}"#, SCHEMA_VERSION + 1)
    }

    #[test]
    fn empty_storage_starts_from_the_seed_without_a_notice() {
        let loaded = load(&MemoryStorage::default());
        assert_eq!(loaded.doc, seed_doc());
        assert_eq!(loaded.notice, None);
        assert!(loaded.can_save);
    }

    #[test]
    fn a_saved_document_loads_back_unchanged() {
        let storage = MemoryStorage::default();
        let mut doc = seed_doc();
        doc.delete_class(doc.classes()[0].id());
        save(&storage, &doc).unwrap();

        let loaded = load(&storage);
        assert_eq!(loaded.doc, doc);
        assert_eq!(loaded.notice, None);
        assert!(loaded.can_save);
    }

    #[test]
    fn an_unreadable_blob_is_copied_aside_before_the_seed_replaces_it() {
        let storage = MemoryStorage::with(KEY, "{not json");
        let loaded = load(&storage);

        assert_eq!(loaded.doc, seed_doc());
        assert_eq!(loaded.notice, Some(Notice::StartedFresh));
        assert!(loaded.can_save);
        assert_eq!(storage.entry(BACKUP_KEY).as_deref(), Some("{not json"));
        assert_eq!(storage.entry(KEY).as_deref(), Some("{not json"));
    }

    #[test]
    fn a_failed_backup_turns_saving_off_so_the_only_copy_survives() {
        let storage = MemoryStorage {
            fail_writes_to: Some(BACKUP_KEY),
            ..MemoryStorage::with(KEY, "{not json")
        };
        let loaded = load(&storage);

        assert_eq!(
            loaded.notice,
            Some(Notice::BackupFailed("quota exceeded".to_owned()))
        );
        assert!(!loaded.can_save);
        assert_eq!(storage.entry(KEY).as_deref(), Some("{not json"));
    }

    #[test]
    fn a_newer_blob_is_never_touched_and_turns_saving_off() {
        let storage = MemoryStorage::with(KEY, &newer_blob());
        storage
            .entries
            .borrow_mut()
            .insert(BACKUP_KEY.to_owned(), "older backup".to_owned());
        let loaded = load(&storage);

        assert_eq!(loaded.notice, Some(Notice::OutOfDate));
        assert!(!loaded.can_save);
        assert!(storage.writes.borrow().is_empty());
        assert_eq!(storage.entry(KEY), Some(newer_blob()));
        assert_eq!(storage.entry(BACKUP_KEY).as_deref(), Some("older backup"));
    }

    #[test]
    fn unreadable_storage_turns_saving_off() {
        let storage = MemoryStorage {
            fail_reads: true,
            ..MemoryStorage::default()
        };
        let loaded = load(&storage);

        assert_eq!(
            loaded.notice,
            Some(Notice::StorageUnreadable("read blocked".to_owned()))
        );
        assert!(!loaded.can_save);
        assert!(storage.writes.borrow().is_empty());
    }

    #[test]
    fn a_rejected_save_returns_the_cause() {
        let storage = MemoryStorage {
            fail_writes_to: Some(KEY),
            ..MemoryStorage::default()
        };
        let error = save(&storage, &seed_doc()).unwrap_err();
        assert_eq!(error.to_string(), "quota exceeded");
    }
}
