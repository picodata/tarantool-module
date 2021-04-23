use std::cmp::Ordering;

use failure::Fail;
use raft::prelude::{ConfState, Entry, HardState, Snapshot, SnapshotMetadata};
use raft::storage::Storage as RaftStorage;
use raft::{Error as RaftError, RaftState, StorageError};

use crate::error::Error;
use crate::index::{IndexFieldType, IndexOptions, IndexPart, IndexType, IteratorType};
use crate::space::{Space, SpaceCreateOptions, SpaceFieldFormat, SpaceFieldType};
use crate::tuple::{AsTuple, Tuple};

pub struct Storage {
    raft_state: RaftState,
    snapshot_metadata: SnapshotMetadata,
    log_space: Space,
}

impl Storage {
    pub fn new() -> Result<Self, Error> {
        let log_space_name = "_log";
        let log_space = match Space::find(log_space_name) {
            None => {
                let log_space = Space::create(
                    log_space_name,
                    &SpaceCreateOptions {
                        format: Some(vec![
                            SpaceFieldFormat::new("index", SpaceFieldType::Unsigned),
                            SpaceFieldFormat::new("term", SpaceFieldType::Unsigned),
                            SpaceFieldFormat::new("data", SpaceFieldType::String),
                        ]),
                        is_temporary: false,
                        ..Default::default()
                    },
                )?;

                log_space.create_index(
                    "primary",
                    &IndexOptions {
                        index_type: Some(IndexType::Tree),
                        parts: Some(vec![
                            IndexPart::new(1, IndexFieldType::Unsigned),
                            // IndexPart::new(2, IndexFieldType::Unsigned),
                        ]),
                        unique: Some(true),
                        ..Default::default()
                    },
                );
                log_space
            }
            Some(log_space) => log_space,
        };

        let mut raft_state = RaftState::default();
        if let Some(last_entry) = log_space.primary_key().max(&())? {
            let last_entry: LogRecord = last_entry.into_struct()?;
            raft_state.hard_state.commit = last_entry.index;
            raft_state.hard_state.term = last_entry.term;
        }

        Ok(Storage {
            raft_state,
            snapshot_metadata: Default::default(),
            log_space,
        })
    }

    pub fn apply_snapshot(&mut self, snapshot: Snapshot) -> Result<(), Error> {
        unimplemented!()
    }

    pub fn append(&mut self, entries: &[Entry]) -> Result<(), Error> {
        if entries.is_empty() {
            return Ok(());
        }

        let first_index = self.first_index()?;
        let last_index = self.last_index()?;

        if first_index > entries[0].index {
            panic!(
                "overwrite compacted raft logs, compacted: {}, append: {}",
                first_index, entries[0].index,
            );
        }

        if last_index + 1 < entries[0].index {
            panic!(
                "raft logs should be continuous, last index: {}, new appended: {}",
                last_index, entries[0].index,
            );
        }

        for entry in entries {
            self.log_space.replace(&LogRecord {
                index: entry.index,
                term: entry.term,
                data: Some("".to_string()),
            })?;
        }

        Ok(())
    }

    pub fn set_hard_state(&mut self, hs: HardState) -> Result<(), Error> {
        self.raft_state.hard_state = hs;
        Ok(())
    }

    pub fn set_conf_state(&mut self, conf_state: ConfState) -> Result<(), Error> {
        self.raft_state.conf_state = conf_state;
        Ok(())
    }

    pub fn set_last_apply_index(&mut self, index: u64) -> Result<(), Error> {
        unimplemented!()
    }
}

impl RaftStorage for Storage {
    fn initial_state(&self) -> Result<RaftState, RaftError> {
        Ok(self.raft_state.clone())
    }

    fn entries(
        &self,
        low: u64,
        high: u64,
        max_size: impl Into<Option<u64>>,
    ) -> Result<Vec<Entry>, RaftError> {
        let max_size = max_size.into();
        if low < self.first_index()? {
            return Err(RaftError::Store(StorageError::Compacted));
        }

        let last_index = self.last_index()?;
        if high > last_index + 1 {
            panic!(
                "index out of bound (last: {}, high: {})",
                last_index + 1,
                high
            );
        }

        let hi = high - 1;

        let result = self
            .log_space
            .primary_key()
            .select(IteratorType::GE, &(low,))
            .and_then(|log_data| {
                let mut result = vec![];
                for log_record in log_data {
                    let log_record = log_record.into_struct::<LogRecord>()?;
                    if log_record.index > high {
                        break;
                    }

                    let mut entry = Entry::default();
                    entry.index = log_record.index;
                    entry.term = log_record.term;
                    result.push(entry)
                }
                Ok(result)
            });
        result.map_err(|e| return RaftError::Store(StorageError::Other(Box::new(e.compat()))))
    }

    fn term(&self, idx: u64) -> Result<u64, RaftError> {
        if idx == self.snapshot_metadata.index {
            return Ok(self.snapshot_metadata.term);
        }

        if idx < self.first_index()? {
            return Err(RaftError::Store(StorageError::Compacted));
        }

        if idx > self.last_index()? {
            return Err(RaftError::Store(StorageError::Unavailable));
        }

        let result = self
            .log_space
            .get(&(idx,))
            .and_then(|result| result.unwrap().into_struct::<(u64, u64)>().map(|val| val.1));
        result.map_err(|e| return RaftError::Store(StorageError::Other(Box::new(e.compat()))))
    }

    fn first_index(&self) -> Result<u64, RaftError> {
        let result = self
            .log_space
            .primary_key()
            .min(&())
            .and_then(|result| match result {
                None => Ok(self.snapshot_metadata.index + 1),
                Some(entry) => entry.into_struct::<(u64,)>().map(|val| val.0),
            });
        result.map_err(|e| return RaftError::Store(StorageError::Other(Box::new(e.compat()))))
    }

    fn last_index(&self) -> Result<u64, RaftError> {
        let result = self
            .log_space
            .primary_key()
            .max(&())
            .and_then(|result| match result {
                None => Ok(self.snapshot_metadata.index),
                Some(entry) => entry.into_struct::<(u64,)>().map(|val| val.0),
            });
        result.map_err(|e| return RaftError::Store(StorageError::Other(Box::new(e.compat()))))
    }

    fn snapshot(&self, request_index: u64) -> Result<Snapshot, RaftError> {
        let mut snapshot = Snapshot::default();
        {
            let meta = snapshot.mut_metadata();
            meta.index = self.raft_state.hard_state.commit;
            meta.term = match meta.index.cmp(&self.snapshot_metadata.index) {
                Ordering::Equal => self.snapshot_metadata.term,
                Ordering::Greater => {
                    let result = self.log_space.get(&(meta.index,)).and_then(|result| {
                        result.unwrap().into_struct::<(u64, u64)>().map(|val| val.1)
                    });
                    result.map_err(|e| {
                        return RaftError::Store(StorageError::Other(Box::new(e.compat())));
                    })?
                }
                Ordering::Less => {
                    panic!();
                }
            };
            meta.set_conf_state(self.raft_state.conf_state.clone());
        }

        if snapshot.get_metadata().index < request_index {
            snapshot.mut_metadata().index = request_index;
        }
        Ok(snapshot)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct LogRecord {
    index: u64,
    term: u64,
    data: Option<String>,
}

impl AsTuple for LogRecord {}
