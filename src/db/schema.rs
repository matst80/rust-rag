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
    ensure_column_exists(connection, "items", "path", "TEXT")?;
    connection.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_items_path ON items(path);",
    )?;
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
         );",
    )?;

    connection.execute_batch(&format!(
        "
        CREATE VIRTUAL TABLE IF NOT EXISTS vec_items USING vec0(
            id TEXT PRIMARY KEY,
            embedding FLOAT[{embedding_dimension}]
        );
        "
    ))?;

    // Code-repo ingestion tables. Dim 1536 = BGE-Code-v1.
    // sqlite-vec vec0 needs static dim per virtual table → separate from vec_items.
    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS code_repos (
            id              TEXT PRIMARY KEY,
            name            TEXT NOT NULL UNIQUE,
            root_path       TEXT NOT NULL,
            include_globs   TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(include_globs)),
            exclude_globs   TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(exclude_globs)),
            enabled         INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0,1)),
            default_branch  TEXT,
            created_at      INTEGER NOT NULL,
            updated_at      INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS code_files (
            id              TEXT PRIMARY KEY,
            repo_id         TEXT NOT NULL REFERENCES code_repos(id) ON DELETE CASCADE,
            repo_name       TEXT NOT NULL,
            path            TEXT NOT NULL,
            basename        TEXT NOT NULL,
            dir             TEXT NOT NULL DEFAULT '',
            extension       TEXT,
            language        TEXT,
            size_bytes      INTEGER NOT NULL DEFAULT 0,
            line_count      INTEGER NOT NULL DEFAULT 0,
            git_sha         TEXT,
            git_branch      TEXT,
            content_hash    TEXT NOT NULL,
            mtime           INTEGER,
            indexed_at      INTEGER NOT NULL,
            summary         TEXT,
            role            TEXT,
            imports         TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(imports)),
            outline         TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(outline)),
            todos           TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(todos)),
            created_at      INTEGER NOT NULL,
            updated_at      INTEGER NOT NULL
        );

        CREATE UNIQUE INDEX IF NOT EXISTS idx_code_files_repo_path ON code_files(repo_id, path);
        CREATE INDEX IF NOT EXISTS idx_code_files_basename ON code_files(basename);
        CREATE INDEX IF NOT EXISTS idx_code_files_language ON code_files(language);
        CREATE INDEX IF NOT EXISTS idx_code_files_repo_dir ON code_files(repo_id, dir);

        CREATE TABLE IF NOT EXISTS code_chunks (
            id                 TEXT PRIMARY KEY,
            file_id            TEXT NOT NULL REFERENCES code_files(id) ON DELETE CASCADE,
            repo_id            TEXT NOT NULL REFERENCES code_repos(id) ON DELETE CASCADE,
            repo_name          TEXT NOT NULL,
            path               TEXT NOT NULL,
            basename           TEXT NOT NULL,
            language           TEXT,
            ordinal            INTEGER NOT NULL,
            start_line         INTEGER NOT NULL,
            end_line           INTEGER NOT NULL,
            byte_start         INTEGER NOT NULL DEFAULT 0,
            byte_end           INTEGER NOT NULL DEFAULT 0,
            symbol_kind        TEXT,
            symbol_name        TEXT,
            symbol_path        TEXT,
            parent_symbol      TEXT,
            visibility         TEXT,
            doc_comment        TEXT,
            signature          TEXT,
            is_test            INTEGER NOT NULL DEFAULT 0 CHECK (is_test IN (0,1)),
            is_public          INTEGER NOT NULL DEFAULT 0 CHECK (is_public IN (0,1)),
            calls              TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(calls)),
            content            TEXT NOT NULL,
            content_hash       TEXT NOT NULL,
            token_count        INTEGER,
            file_content_hash  TEXT NOT NULL,
            git_sha            TEXT,
            prev_chunk_id      TEXT,
            next_chunk_id      TEXT,
            embedding_model    TEXT NOT NULL,
            embedding_version  INTEGER NOT NULL,
            created_at         INTEGER NOT NULL,
            updated_at         INTEGER NOT NULL
        );

        CREATE UNIQUE INDEX IF NOT EXISTS idx_code_chunks_file_ordinal ON code_chunks(file_id, ordinal);
        CREATE INDEX IF NOT EXISTS idx_code_chunks_repo_path ON code_chunks(repo_id, path, start_line);
        CREATE INDEX IF NOT EXISTS idx_code_chunks_basename ON code_chunks(basename);
        CREATE INDEX IF NOT EXISTS idx_code_chunks_symbol ON code_chunks(repo_id, symbol_name);
        CREATE INDEX IF NOT EXISTS idx_code_chunks_language ON code_chunks(language);
        CREATE INDEX IF NOT EXISTS idx_code_chunks_is_test ON code_chunks(is_test);
        CREATE INDEX IF NOT EXISTS idx_code_chunks_is_public ON code_chunks(is_public);
        CREATE INDEX IF NOT EXISTS idx_code_files_role ON code_files(role);
        ",
    )?;

    connection.execute_batch(
        "
        CREATE VIRTUAL TABLE IF NOT EXISTS vec_code_chunks USING vec0(
            id TEXT PRIMARY KEY,
            embedding FLOAT[1536]
        );
        ",
    )?;

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
