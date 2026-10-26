//! Durable runtime journal, separate from the original synthetic M0 tables.
use crate::{Error, Result, Store};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use std::collections::BTreeMap;
use sync_core::mirror::{Entry, Link, Operation};

#[derive(Clone, Debug)]
pub struct Intent {
    pub operation: Operation,
    pub running: bool,
}
impl Store {
    pub fn init_mirror(&mut self) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let version: i64 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version == 5 {
            return Ok(());
        }
        if !(1..=4).contains(&version) {
            return Err(Error::UnsupportedSchema(version));
        }
        if version == 1 {
            tx.execute_batch("CREATE TABLE mirror_links(id TEXT PRIMARY KEY NOT NULL,payload TEXT NOT NULL) STRICT;
            CREATE TABLE mirror_journal(id TEXT PRIMARY KEY NOT NULL,object_id TEXT NOT NULL,payload TEXT NOT NULL,phase TEXT NOT NULL CHECK(phase IN ('planned','running'))) STRICT;
            CREATE UNIQUE INDEX mirror_active ON mirror_journal(object_id);
            CREATE TABLE mirror_history(seq INTEGER PRIMARY KEY,operation_id TEXT NOT NULL,kind TEXT NOT NULL,result TEXT NOT NULL) STRICT; PRAGMA user_version=2;")?;
        }
        if version <= 3 {
            tx.execute_batch("CREATE TABLE upload_sessions(object_id TEXT PRIMARY KEY NOT NULL,payload TEXT NOT NULL) STRICT;")?;
        }
        tx.execute_batch("CREATE TABLE remote_inventory(object_id TEXT PRIMARY KEY NOT NULL,payload TEXT NOT NULL) STRICT;
        CREATE TABLE remote_cursor(singleton INTEGER PRIMARY KEY CHECK(singleton=1),token TEXT NOT NULL) STRICT;")?;
        tx.pragma_update(None, "user_version", 5)?;
        tx.commit()?;
        Ok(())
    }
    pub fn remote_snapshot(&self) -> Result<Option<(String, BTreeMap<String, Entry>)>> {
        let cursor = self
            .connection
            .query_row(
                "SELECT token FROM remote_cursor WHERE singleton=1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(cursor) = cursor else {
            return Ok(None);
        };
        let mut query = self
            .connection
            .prepare("SELECT object_id,payload FROM remote_inventory ORDER BY object_id")?;
        let rows = query.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut inventory = BTreeMap::new();
        for row in rows {
            let (id, payload) = row?;
            inventory.insert(id, serde_json::from_str(&payload)?);
        }
        Ok(Some((cursor, inventory)))
    }
    /// Commit inventory and cursor together; advancing the cursor alone could lose changes.
    pub fn save_remote_snapshot(
        &mut self,
        cursor: &str,
        inventory: &BTreeMap<String, Entry>,
    ) -> Result<()> {
        if cursor.is_empty() {
            return Err(Error::InvalidTransition);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("DELETE FROM remote_inventory", [])?;
        for (id, entry) in inventory {
            tx.execute(
                "INSERT INTO remote_inventory(object_id,payload) VALUES(?1,?2)",
                params![id, serde_json::to_string(entry)?],
            )?;
        }
        tx.execute(
            "INSERT INTO remote_cursor(singleton,token) VALUES(1,?1) ON CONFLICT(singleton) DO UPDATE SET token=excluded.token",
            [cursor],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn advance_remote_cursor(&mut self, cursor: &str) -> Result<()> {
        if cursor.is_empty() {
            return Err(Error::InvalidTransition);
        }
        if self.connection.execute(
            "UPDATE remote_cursor SET token=?1 WHERE singleton=1",
            [cursor],
        )? != 1
        {
            return Err(Error::InvalidTransition);
        }
        Ok(())
    }
    pub fn update_remote_snapshot(
        &mut self,
        cursor: &str,
        updates: &BTreeMap<String, Option<Entry>>,
    ) -> Result<()> {
        if cursor.is_empty() {
            return Err(Error::InvalidTransition);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for (id, entry) in updates {
            if let Some(entry) = entry {
                tx.execute(
                    "INSERT INTO remote_inventory(object_id,payload) VALUES(?1,?2) ON CONFLICT(object_id) DO UPDATE SET payload=excluded.payload",
                    params![id, serde_json::to_string(entry)?],
                )?;
            } else {
                tx.execute("DELETE FROM remote_inventory WHERE object_id=?1", [id])?;
            }
        }
        if tx.execute(
            "UPDATE remote_cursor SET token=?1 WHERE singleton=1",
            [cursor],
        )? != 1
        {
            return Err(Error::InvalidTransition);
        }
        tx.commit()?;
        Ok(())
    }
    pub fn links(&self) -> Result<Vec<Link>> {
        let mut q = self
            .connection
            .prepare("SELECT payload FROM mirror_links ORDER BY id")?;
        let rows = q.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }
    pub fn intents(&self) -> Result<Vec<Intent>> {
        let mut q = self
            .connection
            .prepare("SELECT payload,phase FROM mirror_journal ORDER BY id")?;
        let rows = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.map(|row| {
            let (payload, phase) = row?;
            Ok(Intent {
                operation: serde_json::from_str(&payload)?,
                running: phase == "running",
            })
        })
        .collect()
    }
    pub fn stage(&mut self, op: &Operation) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old: Option<String> = tx
            .query_row(
                "SELECT payload FROM mirror_links WHERE id=?1",
                [&op.object_id],
                |r| r.get(0),
            )
            .optional()?;
        let old: Option<Link> = old.as_deref().map(serde_json::from_str).transpose()?;
        if old != op.base {
            return Err(Error::StaleBase);
        }
        tx.execute(
            "INSERT INTO mirror_journal(id,object_id,payload,phase) VALUES(?1,?2,?3,'planned')",
            params![op.operation_id, op.object_id, serde_json::to_string(op)?],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn mark_running(&mut self, id: &str) -> Result<()> {
        if self.connection.execute(
            "UPDATE mirror_journal SET phase='running' WHERE id=?1 AND phase='planned'",
            [id],
        )? != 1
        {
            return Err(Error::InvalidTransition);
        }
        Ok(())
    }
    /// Executor has inspected the external result before calling this method.
    pub fn finish(&mut self, op: &Operation, link: Option<&Link>, result: &str) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let saved: Option<String> = tx
            .query_row(
                "SELECT payload FROM mirror_journal WHERE id=?1",
                [&op.operation_id],
                |r| r.get(0),
            )
            .optional()?;
        let saved: Operation = serde_json::from_str(&saved.ok_or(Error::InvalidTransition)?)?;
        if saved != *op || link.is_some_and(|l| l.id != op.object_id) {
            return Err(Error::InvalidTransition);
        }
        if let Some(link) = link {
            tx.execute("INSERT INTO mirror_links(id,payload) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",params![link.id,serde_json::to_string(link)?])?;
        } else {
            tx.execute("DELETE FROM mirror_links WHERE id=?1", [&op.object_id])?;
        }
        tx.execute(
            "INSERT INTO mirror_history(operation_id,kind,result) VALUES(?1,?2,?3)",
            params![op.operation_id, format!("{:?}", op.kind), result],
        )?;
        tx.execute(
            "DELETE FROM upload_sessions WHERE object_id=?1",
            [&op.object_id],
        )?;
        tx.execute("DELETE FROM mirror_journal WHERE id=?1", [&op.operation_id])?;
        tx.execute("DELETE FROM mirror_history WHERE seq NOT IN (SELECT seq FROM mirror_history ORDER BY seq DESC LIMIT 1000)",[])?;
        tx.commit()?;
        Ok(())
    }
    pub fn abandon_unstarted(&mut self, op: &Operation) -> Result<()> {
        if self.connection.execute(
            "DELETE FROM mirror_journal WHERE id=?1 AND phase='planned'",
            [&op.operation_id],
        )? != 1
        {
            return Err(Error::InvalidTransition);
        }
        Ok(())
    }
    pub fn history(&self) -> Result<Vec<String>> {
        let mut q = self
            .connection
            .prepare("SELECT kind||': '||result FROM mirror_history ORDER BY seq DESC LIMIT 100")?;
        Ok(q.query_map([], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<_, _>>()?)
    }
}

impl Store {
    /// Explicit operator recovery: retire an intent while retaining the old base.
    pub fn retire(&mut self, op: &Operation, reason: &str) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let saved: Option<String> = tx
            .query_row(
                "SELECT payload FROM mirror_journal WHERE id=?1",
                [&op.operation_id],
                |r| r.get(0),
            )
            .optional()?;
        let saved: Operation = serde_json::from_str(&saved.ok_or(Error::InvalidTransition)?)?;
        if saved != *op {
            return Err(Error::InvalidTransition);
        }
        tx.execute(
            "INSERT INTO mirror_history(operation_id,kind,result) VALUES(?1,?2,?3)",
            params![op.operation_id, format!("{:?}", op.kind), reason],
        )?;
        tx.execute(
            "DELETE FROM upload_sessions WHERE object_id=?1",
            [&op.object_id],
        )?;
        tx.execute("DELETE FROM mirror_journal WHERE id=?1", [&op.operation_id])?;
        tx.commit()?;
        Ok(())
    }
}

impl Store {
    pub fn upload_session(
        &self,
        object_id: &str,
    ) -> Result<Option<sync_core::mirror::UploadSession>> {
        let value: Option<String> = self
            .connection
            .query_row(
                "SELECT payload FROM upload_sessions WHERE object_id=?1",
                [object_id],
                |row| row.get(0),
            )
            .optional()?;
        value
            .map(|value| serde_json::from_str(&value).map_err(Error::from))
            .transpose()
    }
    pub fn save_upload_session(
        &mut self,
        object_id: &str,
        session: &sync_core::mirror::UploadSession,
    ) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let running: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM mirror_journal WHERE object_id=?1 AND phase='running')",
            [object_id],
            |row| row.get(0),
        )?;
        if !running {
            return Err(Error::InvalidTransition);
        }
        tx.execute("INSERT INTO upload_sessions(object_id,payload) VALUES(?1,?2) ON CONFLICT(object_id) DO UPDATE SET payload=excluded.payload",
            params![object_id, serde_json::to_string(session)?])?;
        tx.commit()?;
        Ok(())
    }
    pub fn clear_upload_session(&mut self, object_id: &str) -> Result<()> {
        self.connection.execute(
            "DELETE FROM upload_sessions WHERE object_id=?1",
            [object_id],
        )?;
        Ok(())
    }
}
