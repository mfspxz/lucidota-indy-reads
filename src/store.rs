//! Chat persistence: the whole conversation system gets its own schema.
//!
//! Every speaker turn is timestamped (millisecond precision) and attributed.
//! Claims, evidence, and receipts persist alongside the dialogue so the audit
//! trail survives restarts. SQLite, single file, no server.

use rusqlite::{params, Connection, Result};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS speakers (
  id          TEXT PRIMARY KEY,
  label       TEXT NOT NULL,
  kind        TEXT NOT NULL,   -- user | agent | system | mcp_client
  first_seen_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS sessions (
  id            TEXT PRIMARY KEY,
  started_at_ms INTEGER NOT NULL,
  ended_at_ms   INTEGER,
  metadata      TEXT NOT NULL DEFAULT '{}'
);
CREATE TABLE IF NOT EXISTS turns (
  id              TEXT PRIMARY KEY,
  session_id      TEXT NOT NULL REFERENCES sessions(id),
  speaker_id      TEXT NOT NULL REFERENCES speakers(id),
  started_at_ms   INTEGER NOT NULL,
  ended_at_ms     INTEGER NOT NULL,
  text            TEXT NOT NULL,
  intent          TEXT NOT NULL,
  epistemic_state TEXT,
  claim_id        TEXT,
  receipt_id      TEXT,
  latency_ms      INTEGER NOT NULL,
  model           TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_turns_session ON turns(session_id, started_at_ms);
CREATE TABLE IF NOT EXISTS claims (
  id              TEXT PRIMARY KEY,
  session_id      TEXT NOT NULL,
  text            TEXT NOT NULL,
  epistemic_state TEXT NOT NULL,
  provenance      TEXT NOT NULL,
  created_at_ms   INTEGER NOT NULL,
  updated_at_ms   INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS evidence (
  id          TEXT PRIMARY KEY,
  claim_id    TEXT,
  session_id  TEXT NOT NULL,
  text        TEXT NOT NULL,
  kind        TEXT NOT NULL,
  status      TEXT NOT NULL,
  source      TEXT NOT NULL,
  fixture     INTEGER NOT NULL,
  created_at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS receipts (
  receipt_id      TEXT PRIMARY KEY,
  request_id      TEXT NOT NULL,
  session_id      TEXT NOT NULL,
  timestamp_ms    INTEGER NOT NULL,
  actor           TEXT NOT NULL,
  operation       TEXT NOT NULL,
  input_hash      TEXT NOT NULL,
  decision        TEXT NOT NULL,
  epistemic_status TEXT,
  output_hash     TEXT NOT NULL,
  duration_ms     INTEGER NOT NULL,
  success         INTEGER NOT NULL,
  prev_hash       TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_receipts_session ON receipts(session_id, timestamp_ms);
CREATE TABLE IF NOT EXISTS hypotheses (
  id TEXT PRIMARY KEY,
  claim_id TEXT,
  session_id TEXT NOT NULL,
  text TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'HYPOTHESIS',
  created_at_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_evidence_claim ON evidence(claim_id);
"#;

/// One diarization row: (speaker_name_or_label, text, started_at_ms, ended_at_ms, intent).
pub type TranscriptRow = (String, String, i64, i64, String);

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        // Migration: name + attributes for speaker enrollment (idempotent).
        let _ = conn.execute("ALTER TABLE speakers ADD COLUMN name TEXT", []);
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS speaker_attributes (
               speaker_id    TEXT NOT NULL REFERENCES speakers(id),
               key           TEXT NOT NULL,
               value         TEXT NOT NULL,
               updated_at_ms INTEGER NOT NULL,
               PRIMARY KEY (speaker_id, key)
             )",
            [],
        );
        Ok(Self { conn })
    }

    pub fn ensure_speaker(&self, id: &str, label: &str, kind: &str, now_ms: u64) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO speakers (id, label, kind, first_seen_ms) VALUES (?1, ?2, ?3, ?4)",
            params![id, label, kind, now_ms as i64],
        )?;
        Ok(())
    }

    pub fn create_session(&self, id: &str, now_ms: u64) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO sessions (id, started_at_ms) VALUES (?1, ?2)",
            params![id, now_ms as i64],
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_turn(
        &self,
        id: &str,
        session_id: &str,
        speaker_id: &str,
        started_at_ms: u64,
        ended_at_ms: u64,
        text: &str,
        intent: &str,
        epistemic_state: Option<&str>,
        claim_id: Option<&str>,
        receipt_id: Option<&str>,
        latency_ms: u64,
        model: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO turns (id, session_id, speaker_id, started_at_ms, ended_at_ms, text, intent, epistemic_state, claim_id, receipt_id, latency_ms, model)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                id, session_id, speaker_id,
                started_at_ms as i64, ended_at_ms as i64,
                text, intent, epistemic_state, claim_id, receipt_id,
                latency_ms as i64, model
            ],
        )?;
        Ok(())
    }

    pub fn upsert_claim(
        &self,
        id: &str,
        session_id: &str,
        text: &str,
        epistemic_state: &str,
        provenance: &str,
        now_ms: u64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO claims (id, session_id, text, epistemic_state, provenance, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(id) DO UPDATE SET epistemic_state=excluded.epistemic_state, updated_at_ms=excluded.updated_at_ms",
            params![id, session_id, text, epistemic_state, provenance, now_ms as i64],
        )?;
        Ok(())
    }

    // Signatures mirror the SQL INSERT column lists 1:1 by design: grouping
    // the arguments would obscure the schema mapping at each call site.
    #[allow(clippy::too_many_arguments)]
    pub fn record_receipt(
        &self,
        receipt_id: &str,
        request_id: &str,
        session_id: &str,
        timestamp_ms: u64,
        actor: &str,
        operation: &str,
        input_hash: &str,
        decision: &str,
        epistemic_status: Option<&str>,
        output_hash: &str,
        duration_ms: u64,
        success: bool,
        prev_hash: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO receipts
             (receipt_id, request_id, session_id, timestamp_ms, actor, operation, input_hash, decision, epistemic_status, output_hash, duration_ms, success, prev_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                receipt_id, request_id, session_id, timestamp_ms as i64, actor, operation,
                input_hash, decision, epistemic_status, output_hash,
                duration_ms as i64, success as i64, prev_hash
            ],
        )?;
        Ok(())
    }

    // See record_receipt: mirrors the SQL INSERT column list 1:1.
    #[allow(clippy::too_many_arguments)]
    pub fn record_evidence(
        &self,
        id: &str,
        claim_id: Option<&str>,
        session_id: &str,
        text: &str,
        kind: &str,
        status: &str,
        source: &str,
        fixture: bool,
        now_ms: u64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO evidence
             (id, claim_id, session_id, text, kind, status, source, fixture, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id,
                claim_id,
                session_id,
                text,
                kind,
                status,
                source,
                fixture as i64,
                now_ms as i64
            ],
        )?;
        Ok(())
    }

    pub fn list_evidence(&self, claim_id: &str) -> Result<Vec<(String, String, String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, text, kind, status FROM evidence WHERE claim_id = ?1 ORDER BY created_at_ms",
        )?;
        let rows = stmt.query_map(params![claim_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?;
        rows.collect()
    }

    pub fn record_hypothesis(
        &self,
        id: &str,
        claim_id: Option<&str>,
        session_id: &str,
        text: &str,
        now_ms: u64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO hypotheses
             (id, claim_id, session_id, text, status, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, 'HYPOTHESIS', ?5)",
            params![id, claim_id, session_id, text, now_ms as i64],
        )?;
        Ok(())
    }

    /// Continuity lookup: has this exact claim been assessed before?
    pub fn find_claim_by_text(&self, text: &str) -> Result<Option<(String, String, i64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, epistemic_state, updated_at_ms FROM claims WHERE text = ?1")?;
        let mut rows = stmt.query_map(params![text], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.next().transpose()
    }

    /// Diarization view: every speaker turn in a session, time-ordered.
    /// Speaker column shows the enrolled name when one exists, else the raw label.
    pub fn transcript(&self, session_id: &str) -> Result<Vec<TranscriptRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(NULLIF(s.name,''), s.label), t.text, t.started_at_ms, t.ended_at_ms, t.intent
             FROM turns t JOIN speakers s ON s.id = t.speaker_id
             WHERE t.session_id = ?1 ORDER BY t.started_at_ms",
        )?;
        let rows = stmt.query_map(params![session_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?;
        rows.collect()
    }

    /// Bind a human name to a speaker slot. Names are descriptive metadata —
    /// never authority, never a credential.
    pub fn set_speaker_name(&self, id: &str, name: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE speakers SET name = ?1 WHERE id = ?2",
            params![name, id],
        )?;
        Ok(())
    }

    pub fn speaker_name(&self, id: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM speakers WHERE id = ?1")?;
        let mut rows = stmt.query_map(params![id], |r| r.get::<_, Option<String>>(0))?;
        Ok(rows.next().transpose()?.flatten())
    }

    /// Volunteered PII / attributes, keyed per speaker. Stored locally only,
    /// never sent to a model or a remote service.
    pub fn set_speaker_attribute(
        &self,
        id: &str,
        key: &str,
        value: &str,
        now_ms: u64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO speaker_attributes (speaker_id, key, value, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(speaker_id, key) DO UPDATE SET value = excluded.value, updated_at_ms = excluded.updated_at_ms",
            params![id, key, value, now_ms as i64],
        )?;
        Ok(())
    }

    pub fn speaker_attributes(&self, id: &str) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT key, value FROM speaker_attributes WHERE speaker_id = ?1 ORDER BY key",
        )?;
        let rows = stmt.query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect()
    }

    pub fn count_speaker_turns(&self, session_id: &str, speaker_id: &str) -> Result<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM turns WHERE session_id = ?1 AND speaker_id = ?2",
            params![session_id, speaker_id],
            |r| r.get(0),
        )
    }

    pub fn count_turns(&self) -> Result<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM turns", [], |r| r.get(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_and_diarized_transcript() {
        let s = Store::open(":memory:").unwrap();
        s.ensure_speaker("spk0", "SPEAKER_00", "user", 1000)
            .unwrap();
        s.ensure_speaker("indy", "INDY", "agent", 1000).unwrap();
        s.create_session("sess1", 1000).unwrap();
        s.record_turn(
            "t1",
            "sess1",
            "spk0",
            1000,
            2500,
            "is it real?",
            "Question",
            Some("UNKNOWN"),
            None,
            Some("r1"),
            60,
            "fixture",
        )
        .unwrap();
        s.record_turn(
            "t2",
            "sess1",
            "indy",
            2500,
            3100,
            "Unknown.",
            "Converse",
            Some("UNKNOWN"),
            None,
            Some("r2"),
            40,
            "fixture",
        )
        .unwrap();
        let tx = s.transcript("sess1").unwrap();
        assert_eq!(tx.len(), 2);
        assert_eq!(tx[0].0, "SPEAKER_00");
        assert_eq!(tx[1].0, "INDY");
        assert!(tx[0].2 < tx[1].2); // timestamps order the speakers
        assert_eq!(s.count_turns().unwrap(), 2);
    }

    #[test]
    fn test_speaker_name_enrollment_and_attributes() {
        let s = Store::open(":memory:").unwrap();
        s.ensure_speaker("spk0", "SPEAKER_00", "user", 1000)
            .unwrap();
        assert_eq!(s.speaker_name("spk0").unwrap(), None);
        s.set_speaker_name("spk0", "Zack").unwrap();
        assert_eq!(s.speaker_name("spk0").unwrap(), Some("Zack".to_string()));
        s.set_speaker_attribute("spk0", "email", "z@example.com", 2000)
            .unwrap();
        s.set_speaker_attribute("spk0", "phone", "555-0100", 2000)
            .unwrap();
        let attrs = s.speaker_attributes("spk0").unwrap();
        assert_eq!(attrs.len(), 2);
        // Transcript shows the enrolled name, not the raw label.
        s.create_session("sess1", 1000).unwrap();
        s.record_turn(
            "t1",
            "sess1",
            "spk0",
            1000,
            2500,
            "hi",
            "NameEnrollment",
            Some("UNKNOWN"),
            None,
            Some("r1"),
            60,
            "fixture",
        )
        .unwrap();
        let tx = s.transcript("sess1").unwrap();
        assert_eq!(tx[0].0, "Zack");
        assert_eq!(s.count_speaker_turns("sess1", "spk0").unwrap(), 1);
    }
}
