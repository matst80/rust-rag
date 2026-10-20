use anyhow::{Context, Result};
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

pub(super) fn initialize_schema(connection: &Connection, embedding_dimension: usize) -> Result<()> {
    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS items (
            id TEXT PRIMARY KEY,
            text TEXT NOT NULL,
            metadata TEXT NOT NULL CHECK (json_valid(metadata)),
            source_id TEXT NOT NULL DEFAULT 'default',
            created_at INTEGER NOT NULL DEFAULT 0,
            updated_at INTEGER NOT NULL DEFAULT 0
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

        -- NOTE: graph_edges deliberately has NO foreign keys on from/to_item_id.
        -- External endpoints (`ext:{kind}:{key}`) dangle by design: they need no
        -- stored row and are resolved synthetically at read time. Existence of
        -- non-external endpoints is enforced by `add_manual_edge`, and
        -- `delete_item` removes incident edges explicitly (replacing the old
        -- ON DELETE CASCADE behavior).
        CREATE TABLE IF NOT EXISTS graph_edges (
            id TEXT PRIMARY KEY,
            from_item_id TEXT NOT NULL,
            to_item_id TEXT NOT NULL,
            edge_type TEXT NOT NULL CHECK (edge_type IN ('similarity', 'manual')),
            relation TEXT,
            sort_order TEXT NOT NULL DEFAULT '00000000000000000000',
            weight REAL NOT NULL,
            directed INTEGER NOT NULL DEFAULT 0 CHECK (directed IN (0, 1)),
            metadata TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(metadata)),
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
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

        CREATE TABLE IF NOT EXISTS oauth_authorization_codes (
            code TEXT PRIMARY KEY,
            client_id TEXT NOT NULL,
            redirect_uri TEXT NOT NULL,
            code_challenge TEXT NOT NULL,
            challenge_method TEXT NOT NULL CHECK (challenge_method IN ('S256')),
            scope TEXT,
            subject TEXT,
            token_id TEXT REFERENCES mcp_tokens(id) ON DELETE SET NULL,
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL,
            consumed_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_oauth_codes_expires ON oauth_authorization_codes(expires_at);

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

        CREATE TABLE IF NOT EXISTS attachments (
            id TEXT PRIMARY KEY,
            item_id TEXT NOT NULL,
            filename TEXT,
            stored_name TEXT NOT NULL,
            mime TEXT,
            size INTEGER,
            sha256 TEXT,
            created_at INTEGER NOT NULL,
            FOREIGN KEY(item_id) REFERENCES items(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_attachments_item_id ON attachments(item_id);
        CREATE INDEX IF NOT EXISTS idx_attachments_created_at ON attachments(created_at DESC);

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
        "updated_at",
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
        "UPDATE items SET updated_at = created_at WHERE updated_at = 0;
         UPDATE messages SET updated_at = created_at WHERE updated_at = 0;
         CREATE INDEX IF NOT EXISTS idx_messages_kind ON messages(kind);
         CREATE INDEX IF NOT EXISTS idx_messages_updated_at ON messages(updated_at DESC);",
    )?;
    ensure_column_exists(
        connection,
        "items",
        "ontology_status",
        "TEXT NOT NULL DEFAULT 'pending'",
    )?;
    // Dreaming cursor: NULL = the dreaming worker has never reached a terminal
    // decision about this row. `list_items` filters on it so a processed item
    // is never re-picked (and so `keep` advances the cursor too).
    ensure_column_exists(connection, "items", "dreamed_at", "INTEGER")?;
    connection.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_items_dreamed_at ON items(source_id, dreamed_at, created_at);",
    )?;
    ensure_column_exists(connection, "items", "path", "TEXT")?;
    connection.execute_batch("CREATE INDEX IF NOT EXISTS idx_items_path ON items(path);")?;
    ensure_column_exists(
        connection,
        "graph_edges",
        "sort_order",
        "TEXT NOT NULL DEFAULT '00000000000000000000'",
    )?;
    connection.execute_batch(
        "UPDATE graph_edges
         SET sort_order = printf('%020d', COALESCE(updated_at, created_at, 0))
         WHERE sort_order IS NULL OR trim(sort_order) = '';
         CREATE INDEX IF NOT EXISTS idx_graph_edges_sort_order ON graph_edges(sort_order ASC, id ASC);",
    )?;
    drop_legacy_graph_edge_foreign_keys(connection)?;
    ensure_column_exists(connection, "items", "analysis_json", "TEXT")?;
    ensure_column_exists(connection, "items", "analysis_at", "INTEGER")?;
    ensure_column_exists(connection, "items", "analysis_model", "TEXT")?;
    ensure_column_exists(connection, "items", "type", "TEXT")?;
    ensure_column_exists(connection, "items", "data", "TEXT")?;
    connection.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_items_type ON items(type);
         CREATE TABLE IF NOT EXISTS schemas (
             type_name TEXT PRIMARY KEY,
             json_schema TEXT NOT NULL CHECK (json_valid(json_schema)),
             title TEXT,
             description TEXT,
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS user_oauth_credentials (
             subject TEXT NOT NULL,
             provider TEXT NOT NULL,
             access_token_enc TEXT,
             refresh_token_enc TEXT,
             scopes TEXT NOT NULL DEFAULT '',
             expires_at INTEGER,
             account_email TEXT,
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL,
             PRIMARY KEY (subject, provider)
         );
         CREATE INDEX IF NOT EXISTS idx_user_oauth_subject ON user_oauth_credentials(subject);
         CREATE TABLE IF NOT EXISTS push_subscriptions (
             id TEXT PRIMARY KEY,
             subject TEXT NOT NULL,
             endpoint TEXT NOT NULL,
             p256dh TEXT NOT NULL,
             auth TEXT NOT NULL,
             user_agent TEXT,
             created_at INTEGER NOT NULL,
             last_used_at INTEGER,
             UNIQUE (subject, endpoint)
         );
         CREATE INDEX IF NOT EXISTS idx_push_subscriptions_subject
             ON push_subscriptions(subject);
         CREATE TABLE IF NOT EXISTS ontology_predicates (
             name TEXT NOT NULL,
             source_id TEXT NOT NULL DEFAULT '*',
             description TEXT NOT NULL,
             direction TEXT NOT NULL,
             example_from TEXT,
             example_to TEXT,
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL,
             PRIMARY KEY (name, source_id)
         );
         CREATE TABLE IF NOT EXISTS public_shares (
             token TEXT PRIMARY KEY,
             item_id TEXT NOT NULL,
             created_at INTEGER NOT NULL,
             expires_at INTEGER,
             FOREIGN KEY(item_id) REFERENCES items(id) ON DELETE CASCADE
         );
         CREATE INDEX IF NOT EXISTS idx_public_shares_item_id ON public_shares(item_id);
         CREATE INDEX IF NOT EXISTS idx_public_shares_expires_at ON public_shares(expires_at);
                  CREATE TABLE IF NOT EXISTS dream_actions (
                      id TEXT PRIMARY KEY,
                      round_id TEXT NOT NULL,
                      item_id TEXT NOT NULL,
                      action TEXT NOT NULL,
                      target_id TEXT,
                      result_item_id TEXT,
                      reason TEXT NOT NULL DEFAULT '',
                      outcome TEXT NOT NULL,
                      dry_run INTEGER NOT NULL DEFAULT 0 CHECK (dry_run IN (0, 1)),
                      created_at INTEGER NOT NULL
                  );
                  CREATE INDEX IF NOT EXISTS idx_dream_actions_created_at ON dream_actions(created_at DESC);
                  CREATE INDEX IF NOT EXISTS idx_dream_actions_item_id ON dream_actions(item_id);
                  CREATE TABLE IF NOT EXISTS dream_clusters (
                      id TEXT PRIMARY KEY,
                      round_id TEXT NOT NULL,
                      source_id TEXT NOT NULL,
                      label TEXT NOT NULL DEFAULT '',
                      member_count INTEGER NOT NULL DEFAULT 0,
                      vocabulary TEXT NOT NULL DEFAULT '[]',
                      cohesion REAL NOT NULL DEFAULT 0,
                      params TEXT NOT NULL DEFAULT '{}',
                      created_at INTEGER NOT NULL
                  );
                  CREATE INDEX IF NOT EXISTS idx_dream_clusters_round ON dream_clusters(round_id);
                  CREATE INDEX IF NOT EXISTS idx_dream_clusters_source ON dream_clusters(source_id, created_at DESC);
                  CREATE TABLE IF NOT EXISTS dream_cluster_members (
                      cluster_id TEXT NOT NULL,
                      item_id TEXT NOT NULL,
                      created_at INTEGER NOT NULL,
                      PRIMARY KEY (cluster_id, item_id),
                      FOREIGN KEY (cluster_id) REFERENCES dream_clusters(id) ON DELETE CASCADE
                  );
                  CREATE INDEX IF NOT EXISTS idx_dream_cluster_members_item ON dream_cluster_members(item_id);",
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

/// One-time migration for databases created before external edge endpoints
/// were allowed to dangle: rebuild `graph_edges` without its foreign keys
/// (SQLite cannot drop constraints in place), then delete the empty
/// placeholder rows those constraints forced into existence. Edges pointing
/// at the deleted stubs are kept — they dangle and resolve synthetically.
/// Fresh databases built from the DDL above have no FKs, so this is a no-op.
fn drop_legacy_graph_edge_foreign_keys(connection: &Connection) -> Result<()> {
    let fk_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_foreign_key_list('graph_edges')",
            [],
            |row| row.get(0),
        )
        .context("inspecting graph_edges foreign keys")?;
    if fk_count == 0 {
        return Ok(());
    }
    // Foreign-key enforcement cannot change inside a transaction, and the
    // legacy table cannot drop constraints in place, so each step runs as
    // its own statement with enforcement off throughout.
    connection
        .execute_batch("PRAGMA foreign_keys = OFF;")
        .context("disabling foreign keys for graph_edges rebuild")?;
    let rebuild = connection.execute_batch(
        "CREATE TABLE graph_edges_new (
            id TEXT PRIMARY KEY,
            from_item_id TEXT NOT NULL,
            to_item_id TEXT NOT NULL,
            edge_type TEXT NOT NULL CHECK (edge_type IN ('similarity', 'manual')),
            relation TEXT,
            sort_order TEXT NOT NULL DEFAULT '00000000000000000000',
            weight REAL NOT NULL,
            directed INTEGER NOT NULL DEFAULT 0 CHECK (directed IN (0, 1)),
            metadata TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(metadata)),
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        INSERT INTO graph_edges_new
            (id, from_item_id, to_item_id, edge_type, relation, sort_order,
             weight, directed, metadata, created_at, updated_at)
        SELECT id, from_item_id, to_item_id, edge_type, relation, sort_order,
             weight, directed, metadata, created_at, updated_at
        FROM graph_edges;
        DROP TABLE graph_edges;
        ALTER TABLE graph_edges_new RENAME TO graph_edges;
        CREATE INDEX IF NOT EXISTS idx_graph_edges_from ON graph_edges(from_item_id);
        CREATE INDEX IF NOT EXISTS idx_graph_edges_to ON graph_edges(to_item_id);
        CREATE INDEX IF NOT EXISTS idx_graph_edges_type ON graph_edges(edge_type);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_graph_edges_similarity_pair
            ON graph_edges(from_item_id, to_item_id, edge_type)
            WHERE edge_type = 'similarity';
        CREATE INDEX IF NOT EXISTS idx_graph_edges_sort_order ON graph_edges(sort_order ASC, id ASC);",
    );
    rebuild.context("rebuilding graph_edges without foreign keys")?;
    // With the FKs gone the cascade is gone too: drop the empty external
    // stubs (only empty-text rows — never touch real content), keep edges.
    connection
        .execute(
            "DELETE FROM items
             WHERE text = ''
               AND (id LIKE 'ext:%'
                    OR source_id = 'external'
                    OR COALESCE(json_extract(metadata, '$.external'), 0) != 0)",
            [],
        )
        .context("deleting legacy external stub rows")?;
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .context("re-enabling foreign keys after graph_edges rebuild")?;
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
