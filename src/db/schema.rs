use anyhow::Result;
use rusqlite::Connection;
use sqlite_vec::sqlite3_vec_init;
use std::sync::Once;

pub(super) fn register_sqlite_vec() {
    static SQLITE_VEC_INIT: Once = Once::new();

    SQLITE_VEC_INIT.call_once(|| unsafe {
        rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite3_vec_init as *const (),
        )));
    });
}

pub(super) fn initialize_schema(
    connection: &Connection,
    embedding_dimension: usize,
) -> Result<()> {
    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS items (
            id TEXT PRIMARY KEY,
            text TEXT NOT NULL,
            metadata TEXT NOT NULL CHECK (json_valid(metadata)),
            source_id TEXT NOT NULL DEFAULT 'default',
            created_at INTEGER NOT NULL DEFAULT 0
        );

        CREATE INDEX IF NOT EXISTS idx_items_source_id ON items(source_id);
        CREATE INDEX IF NOT EXISTS idx_items_created_at ON items(created_at DESC);

        -- FTS5 virtual table for keyword search
        CREATE VIRTUAL TABLE IF NOT EXISTS items_fts USING fts5(
            id UNINDEXED,
            text,
            content='items',
            content_rowid='rowid'
        );

        -- Triggers to keep FTS in sync with items table
        CREATE TRIGGER IF NOT EXISTS items_ai AFTER INSERT ON items BEGIN
            INSERT INTO items_fts(rowid, id, text) VALUES (new.rowid, new.id, new.text);
        END;
        CREATE TRIGGER IF NOT EXISTS items_ad AFTER DELETE ON items BEGIN
            INSERT INTO items_fts(items_fts, rowid, id, text) VALUES('delete', old.rowid, old.id, old.text);
        END;
        CREATE TRIGGER IF NOT EXISTS items_au AFTER UPDATE ON items BEGIN
            INSERT INTO items_fts(items_fts, rowid, id, text) VALUES('delete', old.rowid, old.id, old.text);
            INSERT INTO items_fts(rowid, id, text) VALUES (new.rowid, new.id, new.text);
        END;

        -- Backfill FTS if it's empty but items has data
        INSERT OR IGNORE INTO items_fts(rowid, id, text)
        SELECT rowid, id, text FROM items WHERE rowid NOT IN (SELECT rowid FROM items_fts);

        CREATE TABLE IF NOT EXISTS graph_edges (
            id TEXT PRIMARY KEY,
            from_item_id TEXT NOT NULL,
            to_item_id TEXT NOT NULL,
            edge_type TEXT NOT NULL CHECK (edge_type IN ('similarity', 'manual')),
            relation TEXT,
            weight REAL NOT NULL,
            directed INTEGER NOT NULL DEFAULT 0 CHECK (directed IN (0, 1)),
            metadata TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(metadata)),
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            FOREIGN KEY(from_item_id) REFERENCES items(id) ON DELETE CASCADE,
            FOREIGN KEY(to_item_id) REFERENCES items(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_graph_edges_from ON graph_edges(from_item_id);
        CREATE INDEX IF NOT EXISTS idx_graph_edges_to ON graph_edges(to_item_id);
        CREATE INDEX IF NOT EXISTS idx_graph_edges_type ON graph_edges(edge_type);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_graph_edges_similarity_pair
            ON graph_edges(from_item_id, to_item_id, edge_type)
            WHERE edge_type = 'similarity';

        CREATE TABLE IF NOT EXISTS user_events (
            id TEXT PRIMARY KEY,
            subject TEXT NOT NULL,
            event_type TEXT NOT NULL CHECK (event_type IN ('search','view','store','chat')),
            query TEXT,
            query_embedding BLOB,
            item_ids TEXT NOT NULL DEFAULT '[]',
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_user_events_subject ON user_events(subject, created_at DESC);

        CREATE TABLE IF NOT EXISTS user_profiles (
            subject TEXT PRIMARY KEY,
            interest_embedding BLOB,
            event_horizon INTEGER NOT NULL DEFAULT 0,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS mcp_tokens (
            id TEXT PRIMARY KEY,
            token_hash TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            subject TEXT,
            created_at INTEGER NOT NULL,
            last_used_at INTEGER,
            expires_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_mcp_tokens_subject ON mcp_tokens(subject);

        CREATE TABLE IF NOT EXISTS device_auth_requests (
            device_code TEXT PRIMARY KEY,
            user_code TEXT NOT NULL UNIQUE,
            status TEXT NOT NULL CHECK (status IN ('pending', 'approved', 'denied', 'expired')),
            token_id TEXT REFERENCES mcp_tokens(id) ON DELETE SET NULL,
            subject TEXT,
            client_name TEXT,
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL,
            interval_secs INTEGER NOT NULL DEFAULT 5,
            last_polled_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_device_auth_status ON device_auth_requests(status);
        CREATE INDEX IF NOT EXISTS idx_device_auth_expires_at ON device_auth_requests(expires_at);

        CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            channel TEXT NOT NULL,
            sender TEXT NOT NULL,
            sender_kind TEXT NOT NULL DEFAULT 'human' CHECK (sender_kind IN ('human','agent','system')),
            text TEXT NOT NULL,
            kind TEXT NOT NULL DEFAULT 'text',
            metadata TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(metadata)),
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_messages_channel_created ON messages(channel, created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_messages_sender_created ON messages(sender, created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_messages_created_at ON messages(created_at DESC);

        ",
    )?;
    ensure_column_exists(
        connection,
        "items",
        "source_id",
        "TEXT NOT NULL DEFAULT 'default'",
    )?;
    ensure_column_exists(
        connection,
        "items",
        "created_at",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column_exists(
        connection,
        "items",
        "access_count",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column_exists(connection, "items", "last_accessed", "INTEGER")?;
    ensure_column_exists(
        connection,
        "messages",
        "kind",
        "TEXT NOT NULL DEFAULT 'text'",
    )?;
    ensure_column_exists(
        connection,
        "messages",
        "metadata",
        "TEXT NOT NULL DEFAULT '{}'",
    )?;
    ensure_column_exists(
        connection,
        "messages",
        "updated_at",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    // Backfill updated_at = created_at for rows that predate the column.
    connection.execute_batch(
        "UPDATE messages SET updated_at = created_at WHERE updated_at = 0;
         CREATE INDEX IF NOT EXISTS idx_messages_kind ON messages(kind);
         CREATE INDEX IF NOT EXISTS idx_messages_updated_at ON messages(updated_at DESC);",
    )?;
    ensure_column_exists(
        connection,
        "items",
        "ontology_status",
        "TEXT NOT NULL DEFAULT 'pending'",
    )?;

    connection.execute_batch(&format!(
        "
        CREATE VIRTUAL TABLE IF NOT EXISTS vec_items USING vec0(
            id TEXT PRIMARY KEY,
            embedding FLOAT[{embedding_dimension}]
        );
        "
    ))?;

    Ok(())
}

fn ensure_column_exists(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<()> {
    if table_has_column(connection, table, column)? {
        return Ok(());
    }

    connection.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    )?;
    Ok(())
}

fn table_has_column(connection: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;

    for row in rows {
        if row? == column {
            return Ok(true);
        }
    }

    Ok(false)
}
