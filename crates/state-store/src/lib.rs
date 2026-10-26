//! Single-root SQLite journal. No network or file-content effects are executed.
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::{collections::BTreeMap, fmt, path::Path, time::Duration};
use sync_core::{FileState, Observation, Transfer};

#[derive(Debug)]
pub enum Error {
    Sql(rusqlite::Error),
    Json(serde_json::Error),
    UnsupportedSchema(i64),
    InvalidTransition,
    ObservationMismatch,
    StaleBase,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sql(_) => f.write_str("SQLite operation failed"),
            Self::Json(_) => f.write_str("Invalid journal payload"),
            Self::UnsupportedSchema(v) => write!(f, "Unsupported schema version: {v}"),
            Self::InvalidTransition => f.write_str("Operation missing or transition invalid"),
            Self::ObservationMismatch => {
                f.write_str("Observed states do not match intended result")
            }
            Self::StaleBase => f.write_str("Confirmed base changed since planning"),
        }
    }
}
impl std::error::Error for Error {}
impl From<rusqlite::Error> for Error {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sql(value)
    }
}
impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Planned,
    Running,
    EffectObserved,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pending {
    pub operation_id: String,
    pub transfer: Transfer,
    pub phase: Phase,
}

pub struct Store {
    connection: Connection,
}
impl Store {
    /// One database per synthetic sync root. Parent directory must already exist.
    /// A future daemon must create its private XDG directory before calling this.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let version: i64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
        match version {
            0 => tx.execute_batch(
                "CREATE TABLE confirmed (
                    object_id TEXT PRIMARY KEY NOT NULL, payload TEXT NOT NULL
                 ) STRICT;
                 CREATE TABLE operations (
                    operation_id TEXT PRIMARY KEY NOT NULL CHECK(length(operation_id)>0),
                    object_id TEXT NOT NULL,
                    payload TEXT NOT NULL,
                    phase TEXT NOT NULL CHECK(phase IN ('planned','running','observed','committed'))
                 ) STRICT;
                 CREATE UNIQUE INDEX one_active_operation ON operations(object_id)
                    WHERE phase != 'committed';
                 PRAGMA user_version=1;",
            )?,
            1..=5 => {}
            other => return Err(Error::UnsupportedSchema(other)),
        }
        tx.commit()?;
        // Conservative rollback-journal configuration; WAL is not required for
        // this single-writer implementation. FULL persists commit boundaries.
        connection.pragma_update(None, "journal_mode", "DELETE")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        Ok(Self { connection })
    }

    pub fn confirmed(&self) -> Result<BTreeMap<String, FileState>> {
        let mut query = self
            .connection
            .prepare("SELECT object_id,payload FROM confirmed ORDER BY object_id")?;
        let rows = query.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut result = BTreeMap::new();
        for row in rows {
            let (id, payload) = row?;
            result.insert(id, serde_json::from_str(&payload)?);
        }
        Ok(result)
    }

    /// Durable intent before any effect. IDs must be stable across retries.
    /// Caller must supply a planner-approved transfer and the planned base.
    pub fn enqueue(
        &mut self,
        operation_id: &str,
        transfer: &Transfer,
        expected_base: Option<&FileState>,
    ) -> Result<()> {
        let payload = serde_json::to_string(transfer)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let saved: Option<String> = tx
            .query_row(
                "SELECT payload FROM confirmed WHERE object_id=?1",
                [&transfer.id],
                |r| r.get(0),
            )
            .optional()?;
        let saved: Option<FileState> = saved.as_deref().map(serde_json::from_str).transpose()?;
        if saved.as_ref() != expected_base {
            return Err(Error::StaleBase);
        }
        tx.execute("INSERT INTO operations(operation_id,object_id,payload,phase) VALUES(?1,?2,?3,'planned')", params![operation_id, transfer.id, payload])?;
        tx.commit()?;
        Ok(())
    }

    pub fn start(&mut self, operation_id: &str) -> Result<()> {
        let changed = self.connection.execute(
            "UPDATE operations SET phase='running' WHERE operation_id=?1 AND phase='planned'",
            [operation_id],
        )?;
        if changed != 1 {
            return Err(Error::InvalidTransition);
        }
        Ok(())
    }

    /// Recovery of Running must inspect both sides, never replay blindly.
    /// This API accepts fresh observations; gathering them is an executor concern.
    pub fn observe(
        &mut self,
        operation_id: &str,
        local: &Observation,
        remote: &Observation,
    ) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let payload: Option<String> = tx
            .query_row(
                "SELECT payload FROM operations WHERE operation_id=?1 AND phase='running'",
                [operation_id],
                |r| r.get(0),
            )
            .optional()?;
        let transfer: Transfer = serde_json::from_str(&payload.ok_or(Error::InvalidTransition)?)?;
        let expected = Observation::Present(transfer.file);
        if local != &expected || remote != &expected {
            return Err(Error::ObservationMismatch);
        }
        tx.execute(
            "UPDATE operations SET phase='observed' WHERE operation_id=?1",
            [operation_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Advances confirmed state and completes intent in the SAME transaction.
    pub fn commit(&mut self, operation_id: &str) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let payload: Option<String> = tx
            .query_row(
                "SELECT payload FROM operations WHERE operation_id=?1 AND phase='observed'",
                [operation_id],
                |r| r.get(0),
            )
            .optional()?;
        let transfer: Transfer = serde_json::from_str(&payload.ok_or(Error::InvalidTransition)?)?;
        tx.execute("INSERT INTO confirmed(object_id,payload) VALUES(?1,?2) ON CONFLICT(object_id) DO UPDATE SET payload=excluded.payload", params![transfer.id, serde_json::to_string(&transfer.file)?])?;
        tx.execute(
            "UPDATE operations SET phase='committed' WHERE operation_id=?1",
            [operation_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn pending(&self) -> Result<Vec<Pending>> {
        let mut query = self.connection.prepare("SELECT operation_id,payload,phase FROM operations WHERE phase!='committed' ORDER BY operation_id")?;
        let rows = query.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            let (operation_id, payload, phase) = row?;
            let phase = match phase.as_str() {
                "planned" => Phase::Planned,
                "running" => Phase::Running,
                "observed" => Phase::EffectObserved,
                _ => return Err(Error::InvalidTransition),
            };
            result.push(Pending {
                operation_id,
                transfer: serde_json::from_str(&payload)?,
                phase,
            });
        }
        Ok(result)
    }
}

pub mod mirror;
