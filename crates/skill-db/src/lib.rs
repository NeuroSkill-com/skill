// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! `skill-db` — DuckDB storage foundation: one shared instance per database file, and
//! the one-time import of the SQLite stores that preceded it.
//!
//! # Why a registry
//!
//! DuckDB locks a database file per *instance*. Opening the same file twice
//! — even from two threads of one process — fails with "Could not set lock
//! on file". SQLite code opened a fresh `Connection` per request, so every
//! store now goes through [`open`], which keeps one instance per path and
//! hands out [`Connection::try_clone`]s. Clones share the instance (buffer
//! pool, catalog, WAL) and are independent connections for concurrency.
//!
//! Each instance is capped (`threads`, `max_memory`) because the daemon keeps
//! several open at once; DuckDB's defaults (all cores, 80% of RAM) are sized
//! for a single analytical process. Extension autoloading is off so nothing
//! is ever fetched over the network.
//!
//! # Migrating from SQLite
//!
//! [`import_sqlite`] copies the tables of an old `*.sqlite` file into the
//! DuckDB tables a store has just created, coercing SQLite's per-value types
//! to each DuckDB column's declared type. Row counts are verified, the import
//! is recorded in `_skill_import`, and only then is the old file renamed to
//! `*.sqlite.bak` — kept, not deleted, so a release can be rolled back.
//!
//! # SQL dialect notes for store code
//!
//! - `INTEGER` is 32-bit in DuckDB: timestamps and ids must be `BIGINT`.
//!   `REAL` is 32-bit: use `DOUBLE`.
//! - No `AUTOINCREMENT`/`last_insert_rowid()`: ids come from a sequence,
//!   `id BIGINT PRIMARY KEY DEFAULT nextval('seq')`, and inserts that need
//!   the id use `RETURNING id`. List the sequence in [`StoreSchema`] so it is
//!   created before the DDL and re-seeded after an import.
//! - `/` on integers is float division (`//` is integer division); `LIKE`
//!   is case-sensitive (`ILIKE` is not); two-argument `max(a, b)` is
//!   `greatest(a, b)`; bare columns outside `GROUP BY` are an error.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

pub use duckdb::types::{ToSql, Value};
pub use duckdb::{params, params_from_iter, Connection, OptionalExt, Row};

/// DuckDB error, re-exported so store APIs don't name the `duckdb` crate.
pub type Error = duckdb::Error;
/// `Result` over [`Error`].
pub type DbResult<T> = duckdb::Result<T>;

/// Threads per database instance. Store queries are small; parallelism
/// inside one query buys little and each thread is a resident OS thread.
const THREADS: i64 = 2;
/// Buffer-pool cap per instance. DuckDB spills beyond this rather than fail.
const MAX_MEMORY: &str = "256MB";

fn config() -> duckdb::Result<duckdb::Config> {
    duckdb::Config::default()
        .threads(THREADS)?
        .max_memory(MAX_MEMORY)?
        .enable_autoload_extension(false)
}

/// Normalised registry key: absolute, with `.`/`..` resolved where the file
/// (or its parent) exists, so two spellings of one path share an instance.
fn key(path: &Path) -> PathBuf {
    if let Ok(p) = path.canonicalize() {
        return p;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => parent
            .canonicalize()
            .map(|p| p.join(name))
            .unwrap_or_else(|_| path.to_path_buf()),
        _ => path.to_path_buf(),
    }
}

/// Idle instances kept open after their last connection is dropped. Per-day
/// EEG databases mean a scan over history touches many files; keeping every
/// instance resident would hold a buffer pool and worker threads per day.
const MAX_IDLE_INSTANCES: usize = 8;

/// A connection handed out by [`open`]. Derefs to [`Connection`]; while any
/// `Conn` for a file is alive its instance stays registered, so a later
/// `open` of the same file always shares it instead of tripping DuckDB's
/// file lock with a second instance.
pub struct Conn {
    inner: Connection,
    _lease: std::sync::Arc<()>,
}

impl Conn {
    /// Another connection to the same instance (independent transaction
    /// state), holding its own lease.
    pub fn try_clone(&self) -> duckdb::Result<Conn> {
        Ok(Conn {
            inner: self.inner.try_clone()?,
            _lease: self._lease.clone(),
        })
    }
}

impl std::ops::Deref for Conn {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        &self.inner
    }
}

impl std::ops::DerefMut for Conn {
    fn deref_mut(&mut self) -> &mut Connection {
        &mut self.inner
    }
}

struct Entry {
    base: Connection,
    lease: std::sync::Arc<()>,
    last_used: std::time::Instant,
}

impl Entry {
    fn idle(&self) -> bool {
        std::sync::Arc::strong_count(&self.lease) == 1
    }
}

static REGISTRY: LazyLock<Mutex<HashMap<PathBuf, Entry>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn registry() -> std::sync::MutexGuard<'static, HashMap<PathBuf, Entry>> {
    REGISTRY.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Close the least recently used idle instances beyond the cap.
fn evict_idle(reg: &mut HashMap<PathBuf, Entry>, keep: &Path) {
    let mut idle: Vec<(std::time::Instant, PathBuf)> = reg
        .iter()
        .filter(|(k, e)| e.idle() && k.as_path() != keep)
        .map(|(k, e)| (e.last_used, k.clone()))
        .collect();
    if idle.len() <= MAX_IDLE_INSTANCES {
        return;
    }
    idle.sort();
    for (_, k) in idle.into_iter().take(reg.len().saturating_sub(MAX_IDLE_INSTANCES)) {
        if reg.get(&k).is_some_and(Entry::idle) {
            reg.remove(&k);
        }
    }
}

/// A connection to the database at `path`, creating the file (and parent
/// directories) if needed. All callers in the process share one instance.
pub fn open(path: &Path) -> duckdb::Result<Conn> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let k = key(path);
    let mut reg = registry();
    if let Some(e) = reg.get_mut(&k) {
        e.last_used = std::time::Instant::now();
        return Ok(Conn {
            inner: e.base.try_clone()?,
            _lease: e.lease.clone(),
        });
    }
    let base = Connection::open_with_flags(&k, config()?)?;
    let lease = std::sync::Arc::new(());
    let conn = Conn {
        inner: base.try_clone()?,
        _lease: lease.clone(),
    };
    reg.insert(
        k.clone(),
        Entry {
            base,
            lease,
            last_used: std::time::Instant::now(),
        },
    );
    evict_idle(&mut reg, &k);
    Ok(conn)
}

/// Number of instances currently registered (open files).
pub fn open_instances() -> usize {
    registry().len()
}

/// Schema of a store: DDL plus the id sequences its inserts draw from.
pub struct StoreSchema<'a> {
    /// `CREATE … IF NOT EXISTS` statements and idempotent migrations.
    pub ddl: &'a str,
    /// Tables to bring over from the legacy SQLite file, in dependency order.
    pub import_tables: &'a [&'a str],
    /// `(sequence, table, id column)` — created, and re-seeded after an import.
    pub sequences: &'a [(&'a str, &'a str, &'a str)],
}

static INITIALIZED: LazyLock<Mutex<std::collections::HashSet<PathBuf>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashSet::new()));

/// [`open`] a store's database, setting it up on the first open per process:
/// run the DDL, create sequences, import `<name>.sqlite` if it is still
/// there, and re-seed sequences past imported ids. Later opens are a clone.
///
/// A failed import is logged and leaves the SQLite file for the next start;
/// the store still opens (empty) rather than taking its feature down.
pub fn open_store(path: &Path, schema: &StoreSchema<'_>) -> anyhow::Result<Conn> {
    let conn = open(path)?;
    let k = key(path);
    let mut init = INITIALIZED.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if init.contains(&k) {
        return Ok(conn);
    }
    // Sequences first: column defaults reference them.
    for (seq, _, _) in schema.sequences {
        ensure_sequence(&conn, seq)?;
    }
    conn.execute_batch(schema.ddl)?;
    match import_sqlite(&conn, &legacy_sqlite_path(path), schema.import_tables) {
        Ok(Some(_)) => {
            for (seq, table, col) in schema.sequences {
                reseed_sequence(&conn, seq, table, col)?;
            }
        }
        Ok(None) => {}
        Err(e) => eprintln!(
            "[duck] import into {} failed, will retry next start: {e:#}",
            path.display()
        ),
    }
    init.insert(k);
    Ok(conn)
}

/// Open only if the database file already exists — for read paths that
/// must not create an empty database as a side effect.
pub fn open_existing(path: &Path) -> Option<duckdb::Result<Conn>> {
    let registered = registry().contains_key(&key(path));
    (registered || path.exists()).then(|| open(path))
}

/// An isolated in-memory database (tests, scratch work).
pub fn open_in_memory() -> duckdb::Result<Connection> {
    Connection::open_in_memory_with_flags(config()?)
}

/// Drop the shared instance for `path` so the file can be deleted or
/// replaced. Returns `false` — and keeps the instance — while any [`Conn`]
/// for it is still alive: dropping it then would let the next `open` start
/// a second instance on a file the first still has locked.
pub fn forget(path: &Path) -> bool {
    let k = key(path);
    let mut reg = registry();
    if reg.get(&k).is_some_and(|e| !e.idle()) {
        return false;
    }
    reg.remove(&k);
    INITIALIZED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&k);
    true
}

/// Remove a database and its WAL. Fails with `WouldBlock` while the
/// database still has open connections.
pub fn remove_db_files(path: &Path) -> std::io::Result<()> {
    if !forget(path) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::WouldBlock,
            format!("{} is in use", path.display()),
        ));
    }
    for p in [path.to_path_buf(), wal_path(path)] {
        match std::fs::remove_file(&p) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// DuckDB's write-ahead log sits next to the database as `<file>.wal`.
pub fn wal_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".wal");
    PathBuf::from(s)
}

/// `labels.duckdb` → `labels.sqlite`: where the pre-DuckDB store lived.
pub fn legacy_sqlite_path(duck_path: &Path) -> PathBuf {
    duck_path.with_extension("sqlite")
}

/// Create `seq` if missing.
pub fn ensure_sequence(conn: &Connection, seq: &str) -> duckdb::Result<()> {
    conn.execute_batch(&format!("CREATE SEQUENCE IF NOT EXISTS {seq} START 1"))
}

/// Advance `seq` so the next `nextval` is past `max(table.column)`.
/// Needed after importing rows that carry their own ids.
///
/// Advances in place (one `nextval` per skipped value, in a single query)
/// rather than recreating the sequence, so it works while a column default
/// `DEFAULT nextval('seq')` depends on it.
pub fn reseed_sequence(conn: &Connection, seq: &str, table: &str, column: &str) -> duckdb::Result<()> {
    let max: i64 = conn.query_row(&format!("SELECT coalesce(max({column}), 0) FROM {table}"), [], |r| {
        r.get(0)
    })?;
    let next: i64 = conn.query_row(&format!("SELECT nextval('{seq}')"), [], |r| r.get(0))?;
    if next <= max {
        let _: Option<i64> = conn.query_row(
            &format!("SELECT max(nextval('{seq}')) FROM range(?)"),
            [max - next],
            |r| r.get(0),
        )?;
    }
    Ok(())
}

/// Next value of `seq` — for inserts that need the id before the row exists.
pub fn next_id(conn: &Connection, seq: &str) -> duckdb::Result<i64> {
    conn.query_row(&format!("SELECT nextval('{seq}')"), [], |r| r.get(0))
}

/// Whether `table` exists in the main schema.
pub fn table_exists(conn: &Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT count(*) FROM information_schema.tables WHERE table_schema = 'main' AND table_name = ?",
        [table],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n > 0)
    .unwrap_or(false)
}

/// `(name, data_type)` of `table`'s columns in declaration order.
pub fn table_columns(conn: &Connection, table: &str) -> duckdb::Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT column_name, data_type FROM information_schema.columns
         WHERE table_schema = 'main' AND table_name = ? ORDER BY ordinal_position",
    )?;
    let rows = stmt.query_map([table], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

/// What [`import_sqlite`] copied.
#[derive(Debug, Clone, Default)]
pub struct ImportReport {
    pub source: PathBuf,
    /// `(table, rows)` per imported table.
    pub tables: Vec<(String, u64)>,
    /// Where the SQLite file was moved, when it was.
    pub backup: Option<PathBuf>,
}

/// Copy `tables` from the SQLite database at `sqlite_path` into the
/// same-named tables of `conn`, once.
///
/// Returns `Ok(None)` when there is nothing to do (no SQLite file, or it was
/// already imported). Columns are matched by name; SQLite columns the DuckDB
/// table lacks are dropped, DuckDB columns the SQLite table lacks get their
/// defaults. Tables missing from the SQLite file are skipped. The DuckDB
/// side runs in one transaction: on any error — including a row-count
/// mismatch — nothing is committed and the SQLite file is left in place.
pub fn import_sqlite(conn: &Connection, sqlite_path: &Path, tables: &[&str]) -> anyhow::Result<Option<ImportReport>> {
    use anyhow::Context;
    if !sqlite_path.exists() {
        return Ok(None);
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _skill_import (
            source      VARCHAR PRIMARY KEY,
            imported_at BIGINT NOT NULL,
            rows        BIGINT NOT NULL
        )",
    )?;
    let source_name = sqlite_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let done: i64 = conn.query_row(
        "SELECT count(*) FROM _skill_import WHERE source = ?",
        [&source_name],
        |r| r.get(0),
    )?;
    if done > 0 {
        return Ok(None);
    }

    let sqlite = rusqlite::Connection::open_with_flags(
        sqlite_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("open {}", sqlite_path.display()))?;
    let _ = sqlite.busy_timeout(std::time::Duration::from_secs(5));

    let mut report = ImportReport {
        source: sqlite_path.to_path_buf(),
        ..Default::default()
    };
    conn.execute_batch("BEGIN TRANSACTION")?;
    let result = (|| -> anyhow::Result<u64> {
        let mut total = 0u64;
        for &table in tables {
            let Some(n) = import_table(&sqlite, conn, table).with_context(|| format!("import table {table}"))? else {
                continue;
            };
            report.tables.push((table.to_string(), n));
            total += n;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        conn.execute(
            "INSERT INTO _skill_import (source, imported_at, rows) VALUES (?, ?, ?)",
            params![source_name, now, total as i64],
        )?;
        Ok(total)
    })();
    match result {
        Ok(_) => conn.execute_batch("COMMIT")?,
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(e);
        }
    }
    drop(sqlite);

    let backup = sqlite_path.with_extension("sqlite.bak");
    let mut moved = std::fs::rename(sqlite_path, &backup).is_ok();
    for suffix in ["-wal", "-shm"] {
        let mut side = sqlite_path.as_os_str().to_os_string();
        side.push(suffix);
        let side = PathBuf::from(side);
        if side.exists() {
            let mut dest = backup.as_os_str().to_os_string();
            dest.push(suffix);
            moved &= std::fs::rename(&side, PathBuf::from(dest)).is_ok();
        }
    }
    report.backup = moved.then_some(backup);
    eprintln!(
        "[duck] imported {} from {}: {:?}",
        source_name,
        sqlite_path.display(),
        report.tables
    );
    Ok(Some(report))
}

/// Copy one table. `None` if the SQLite file has no such table.
fn import_table(sqlite: &rusqlite::Connection, duck: &Connection, table: &str) -> anyhow::Result<Option<u64>> {
    let exists: i64 = sqlite.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |r| r.get(0),
    )?;
    if exists == 0 {
        return Ok(None);
    }
    let sqlite_cols: Vec<String> = {
        let mut stmt = sqlite.prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
        let cols = stmt.query_map([], |r| r.get::<_, String>(1))?;
        cols.collect::<Result<_, _>>()?
    };
    let duck_cols = table_columns(duck, table)?;
    let shared: Vec<(String, String)> = duck_cols
        .into_iter()
        .filter(|(name, _)| sqlite_cols.iter().any(|c| c.eq_ignore_ascii_case(name)))
        .collect();
    if shared.is_empty() {
        anyhow::bail!("no columns in common");
    }

    let select = format!(
        "SELECT {} FROM \"{table}\"",
        shared
            .iter()
            .map(|(n, _)| format!("\"{n}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let names: Vec<&str> = shared.iter().map(|(n, _)| n.as_str()).collect();
    let kinds: Vec<Kind> = shared.iter().map(|(_, t)| Kind::of(t)).collect();
    let mut appender = duck.appender_with_columns(table, &names)?;
    let mut stmt = sqlite.prepare(&select)?;
    let mut rows = stmt.query([])?;
    let mut n = 0u64;
    let mut values: Vec<duckdb::types::Value> = Vec::with_capacity(names.len());
    while let Some(row) = rows.next()? {
        values.clear();
        for (i, kind) in kinds.iter().enumerate() {
            values.push(kind.coerce(row.get_ref(i)?));
        }
        appender.append_row(duckdb::appender_params_from_iter(values.iter()))?;
        n += 1;
    }
    appender.flush()?;
    drop(appender);

    let source_count: i64 = sqlite.query_row(&format!("SELECT count(*) FROM \"{table}\""), [], |r| r.get(0))?;
    let dest_count: i64 = duck.query_row(&format!("SELECT count(*) FROM \"{table}\""), [], |r| r.get(0))?;
    if source_count as u64 != n || dest_count < source_count {
        anyhow::bail!("row count mismatch: sqlite {source_count}, read {n}, duckdb {dest_count}");
    }
    Ok(Some(n))
}

/// Target column family for coercing SQLite's dynamically typed values.
#[derive(Clone, Copy)]
enum Kind {
    Int,
    Float,
    Bool,
    Text,
    Blob,
}

impl Kind {
    fn of(duck_type: &str) -> Self {
        let t = duck_type.to_ascii_uppercase();
        if t.contains("INT") {
            Self::Int
        } else if t == "DOUBLE" || t == "FLOAT" || t == "REAL" || t.starts_with("DECIMAL") {
            Self::Float
        } else if t == "BOOLEAN" {
            Self::Bool
        } else if t == "BLOB" {
            Self::Blob
        } else {
            Self::Text
        }
    }

    fn coerce(self, v: rusqlite::types::ValueRef<'_>) -> duckdb::types::Value {
        use duckdb::types::Value as D;
        use rusqlite::types::ValueRef as S;
        match (self, v) {
            (_, S::Null) => D::Null,
            (Self::Int, S::Integer(i)) => D::BigInt(i),
            (Self::Int, S::Real(f)) => D::BigInt(f as i64),
            (Self::Int, S::Text(t)) => parse_num(t).map_or(D::Null, |f| D::BigInt(f as i64)),
            (Self::Float, S::Integer(i)) => D::Double(i as f64),
            (Self::Float, S::Real(f)) => D::Double(f),
            (Self::Float, S::Text(t)) => parse_num(t).map_or(D::Null, D::Double),
            (Self::Bool, S::Integer(i)) => D::Boolean(i != 0),
            (Self::Bool, S::Real(f)) => D::Boolean(f != 0.0),
            (Self::Bool, S::Text(t)) => D::Boolean(matches!(t, b"1" | b"true" | b"TRUE")),
            (Self::Blob, S::Blob(b) | S::Text(b)) => D::Blob(b.to_vec()),
            (Self::Text, S::Text(t) | S::Blob(t)) => D::Text(String::from_utf8_lossy(t).into_owned()),
            (Self::Text, S::Integer(i)) => D::Text(i.to_string()),
            (Self::Text, S::Real(f)) => D::Text(f.to_string()),
            (_, S::Blob(_)) | (Self::Blob, _) => D::Null,
        }
    }
}

fn parse_num(t: &[u8]) -> Option<f64> {
    std::str::from_utf8(t).ok()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_shares_one_instance_per_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.duckdb");
        let a = open(&path).unwrap();
        // A second plain open of the same file would fail the instance lock.
        let b = open(&dir.path().join(".").join("a.duckdb")).unwrap();
        a.execute_batch("CREATE TABLE t (x BIGINT); INSERT INTO t VALUES (1)")
            .unwrap();
        let n: i64 = b.query_row("SELECT count(*) FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
        let t = std::thread::spawn({
            let c = open(&path).unwrap();
            move || c.query_row("SELECT sum(x) FROM t", [], |r| r.get::<_, i64>(0)).unwrap()
        });
        assert_eq!(t.join().unwrap(), 1);
        // In use: the instance must not be dropped or the file removed.
        assert!(remove_db_files(&path).is_err());
        drop((a, b));
        remove_db_files(&path).unwrap();
        assert!(!path.exists());
        assert!(open_existing(&path).is_none());
    }

    #[test]
    fn idle_instances_are_evicted_live_ones_never() {
        let dir = tempfile::tempdir().unwrap();
        let held = open(&dir.path().join("held.duckdb")).unwrap();
        held.execute_batch("CREATE TABLE t (x BIGINT)").unwrap();
        for i in 0..(MAX_IDLE_INSTANCES + 6) {
            let c = open(&dir.path().join(format!("day{i}.duckdb"))).unwrap();
            c.execute_batch("CREATE TABLE IF NOT EXISTS t (x BIGINT)").unwrap();
        }
        let in_dir = |reg: &HashMap<PathBuf, Entry>| reg.keys().filter(|k| k.starts_with(key(dir.path()))).count();
        assert!(in_dir(&registry()) <= MAX_IDLE_INSTANCES + 2, "idle instances capped");
        // The held one survived eviction: reopening shares it (no lock error).
        let again = open(&dir.path().join("held.duckdb")).unwrap();
        again.execute_batch("INSERT INTO t VALUES (1)").unwrap();
        let n: i64 = held.query_row("SELECT count(*) FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
        // An evicted file reopens cleanly with its data.
        let day0 = open(&dir.path().join("day0.duckdb")).unwrap();
        assert!(table_exists(&day0, "t"));
    }

    #[test]
    fn sequences_continue_after_imported_ids() {
        let conn = open_in_memory().unwrap();
        ensure_sequence(&conn, "t_seq").unwrap();
        conn.execute_batch(
            "CREATE TABLE t (id BIGINT PRIMARY KEY DEFAULT nextval('t_seq'), v VARCHAR);
             INSERT INTO t VALUES (41, 'a')",
        )
        .unwrap();
        reseed_sequence(&conn, "t_seq", "t", "id").unwrap();
        let id: i64 = conn
            .query_row("INSERT INTO t (v) VALUES ('b') RETURNING id", [], |r| r.get(0))
            .unwrap();
        assert_eq!(id, 42);
        // Re-seeding again is a no-op once past the max.
        reseed_sequence(&conn, "t_seq", "t", "id").unwrap();
        let id: i64 = conn
            .query_row("INSERT INTO t (v) VALUES ('c') RETURNING id", [], |r| r.get(0))
            .unwrap();
        assert!(id > 42);
    }

    #[test]
    fn imports_sqlite_once_with_type_coercion() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("store.sqlite");
        {
            let s = rusqlite::Connection::open(&old).unwrap();
            s.execute_batch(
                "CREATE TABLE items (id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER, score REAL,
                                     name TEXT, emb BLOB, dropped TEXT, flag INTEGER);
                 INSERT INTO items (ts, score, name, emb, dropped, flag)
                   VALUES (1793008760123, 0.5, 'alpha', x'0000803f', 'gone', 1);
                 INSERT INTO items (ts, score, name, emb, dropped, flag)
                   VALUES ('1793008760999', '2', 7, NULL, NULL, 0);",
            )
            .unwrap();
        }
        let conn = open(&dir.path().join("store.duckdb")).unwrap();
        conn.execute_batch(
            "CREATE TABLE items (id BIGINT PRIMARY KEY, ts BIGINT, score DOUBLE, name VARCHAR,
                                 emb BLOB, flag BOOLEAN, added VARCHAR DEFAULT 'x')",
        )
        .unwrap();
        let report = import_sqlite(&conn, &old, &["items", "absent"]).unwrap().unwrap();
        assert_eq!(report.tables, vec![("items".to_string(), 2)]);
        assert!(!old.exists() && dir.path().join("store.sqlite.bak").exists());

        type Item = (i64, i64, f64, String, Option<Vec<u8>>, bool, String);
        let rows: Vec<Item> = conn
            .prepare("SELECT id, ts, score, name, emb, flag, added FROM items ORDER BY id")
            .unwrap()
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            rows[0],
            (
                1,
                1_793_008_760_123,
                0.5,
                "alpha".into(),
                Some(vec![0, 0, 0x80, 0x3f]),
                true,
                "x".into()
            )
        );
        assert_eq!(
            rows[1],
            (2, 1_793_008_760_999, 2.0, "7".into(), None, false, "x".into())
        );

        // Second call is a no-op even if a stray sqlite file reappears.
        std::fs::copy(dir.path().join("store.sqlite.bak"), &old).unwrap();
        assert!(import_sqlite(&conn, &old, &["items"]).unwrap().is_none());
    }

    #[test]
    fn failed_import_leaves_sqlite_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("bad.sqlite");
        {
            let s = rusqlite::Connection::open(&old).unwrap();
            s.execute_batch("CREATE TABLE t (id INTEGER, x TEXT); INSERT INTO t VALUES (1, 'a'), (1, 'b');")
                .unwrap();
        }
        let conn = open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE t (id BIGINT PRIMARY KEY, x VARCHAR)")
            .unwrap();
        assert!(import_sqlite(&conn, &old, &["t"]).is_err());
        assert!(old.exists());
        let n: i64 = conn.query_row("SELECT count(*) FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0, "rolled back");
    }
}
