-- Indy_Reads D1 schema. Mirrors src/store.rs (local SQLite) so the two
-- stay compatible. D1 = WHAT EXISTS. Vectorize = WHAT IS SEMANTICALLY
-- RELATED (candidates only). R2 = BIG THINGS (hashes + metadata here).
--
-- Apply: wrangler d1 execute indy-reads --file=cloudflare/d1-schema.sql

-- ---- chat + evidence core (mirrors local store.rs) ----
CREATE TABLE IF NOT EXISTS speakers (
  id TEXT PRIMARY KEY,
  label TEXT NOT NULL,
  kind TEXT NOT NULL,
  first_seen_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS sessions (
  id TEXT PRIMARY KEY,
  started_at_ms INTEGER NOT NULL,
  ended_at_ms INTEGER,
  metadata TEXT NOT NULL DEFAULT '{}'
);
CREATE TABLE IF NOT EXISTS turns (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions(id),
  speaker_id TEXT NOT NULL REFERENCES speakers(id),
  started_at_ms INTEGER NOT NULL,
  ended_at_ms INTEGER NOT NULL,
  text TEXT NOT NULL,
  intent TEXT NOT NULL,
  epistemic_state TEXT,
  claim_id TEXT,
  receipt_id TEXT,
  latency_ms INTEGER NOT NULL,
  model TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_turns_session ON turns(session_id, started_at_ms);
CREATE TABLE IF NOT EXISTS claims (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL,
  text TEXT NOT NULL,
  epistemic_state TEXT NOT NULL,
  provenance TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS evidence (
  id TEXT PRIMARY KEY,
  claim_id TEXT,
  session_id TEXT NOT NULL,
  text TEXT NOT NULL,
  kind TEXT NOT NULL,
  status TEXT NOT NULL,
  source TEXT NOT NULL,
  fixture INTEGER NOT NULL,
  created_at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS receipts (
  receipt_id TEXT PRIMARY KEY,
  request_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  timestamp_ms INTEGER NOT NULL,
  actor TEXT NOT NULL,
  operation TEXT NOT NULL,
  input_hash TEXT NOT NULL,
  decision TEXT NOT NULL,
  epistemic_status TEXT,
  output_hash TEXT NOT NULL,
  duration_ms INTEGER NOT NULL,
  success INTEGER NOT NULL,
  prev_hash TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_receipts_session ON receipts(session_id, timestamp_ms);

-- ---- graph entities (agents are first-class graph nodes) ----
CREATE TABLE IF NOT EXISTS agents (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  purpose TEXT NOT NULL,
  authority_level TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS personas (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  system_prompt_ref TEXT NOT NULL,  -- personas/<id>.md in the repo
  created_at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS voices (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  provider TEXT NOT NULL,           -- alebex | browser | fixture
  provider_voice_id TEXT,
  created_at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS lenses (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  description TEXT NOT NULL,
  emphasis TEXT NOT NULL,           -- what this lens emphasizes in retrieval
  created_at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS entities (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,                -- ACTOR|SESSION|SPEAKER|AGENT|PERSONA|VOICE|LENS|ENTITY|EVENT|TOOL...
  name TEXT NOT NULL,
  attrs TEXT NOT NULL DEFAULT '{}',  -- bounded JSON payload
  created_at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS edges (
  id TEXT PRIMARY KEY,
  from_id TEXT NOT NULL REFERENCES entities(id),
  to_id TEXT NOT NULL REFERENCES entities(id),
  relation TEXT NOT NULL,            -- HAS_PERSONA|REALIZED_BY|SUPPORTED_BY|PRODUCED|AUTHORIZED_BY...
  graph_revision INTEGER NOT NULL,
  created_at_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_edges_from ON edges(from_id, relation);
CREATE INDEX IF NOT EXISTS idx_edges_to ON edges(to_id, relation);
CREATE TABLE IF NOT EXISTS sources (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  ref TEXT NOT NULL,                 -- URI / R2 key / citation
  verified INTEGER NOT NULL DEFAULT 0,
  created_at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS hypotheses (
  id TEXT PRIMARY KEY,
  claim_id TEXT REFERENCES claims(id),
  text TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'HYPOTHESIS',  -- never silently promoted
  created_at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS graph_revisions (
  revision INTEGER PRIMARY KEY,
  created_at_ms INTEGER NOT NULL,
  cause TEXT NOT NULL                -- receipt_id that caused the mutation
);

-- ---- embedding metadata (vectors live in Vectorize, truth lives here) ----
CREATE TABLE IF NOT EXISTS embedding_metadata (
  entity_id TEXT PRIMARY KEY,
  model_id TEXT NOT NULL,            -- e.g. ternlight/base@0.1.0
  vector_dim INTEGER NOT NULL,       -- 384
  embedding_hash TEXT NOT NULL,      -- hash of the vector for reproducibility
  created_at_ms INTEGER NOT NULL
);
