use anyhow::{Context, Result};
use rusqlite::{OptionalExtension, params};

use super::{
    AuthStore, DeviceAuthRecord, DeviceAuthStatus, McpTokenRecord, NewDeviceAuth, NewMcpToken,
    NewOAuthAuthCode, OAuthAuthCodeRecord, SqliteVectorStore,
};

impl AuthStore for SqliteVectorStore {
    fn create_mcp_token(&self, token: NewMcpToken) -> Result<McpTokenRecord> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;

        connection.execute(
            "
            INSERT INTO mcp_tokens (id, token_hash, name, subject, created_at, expires_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ",
            params![
                token.id,
                token.token_hash,
                token.name,
                token.subject,
                token.created_at,
                token.expires_at,
            ],
        )?;

        Ok(McpTokenRecord {
            id: token.id,
            name: token.name,
            subject: token.subject,
            created_at: token.created_at,
            last_used_at: None,
            expires_at: token.expires_at,
        })
    }

    fn find_mcp_token_by_hash(&self, hash: &str) -> Result<Option<McpTokenRecord>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;

        let mut statement = connection.prepare(
            "
            SELECT id, name, subject, created_at, last_used_at, expires_at
            FROM mcp_tokens
            WHERE token_hash = ?1
            ",
        )?;
        let record = statement
            .query_row(params![hash], map_mcp_token_row)
            .optional()?;
        Ok(record)
    }

    fn touch_mcp_token(&self, id: &str, now: i64) -> Result<()> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        connection.execute(
            "UPDATE mcp_tokens SET last_used_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
        Ok(())
    }

    fn list_mcp_tokens(&self, subject: Option<&str>) -> Result<Vec<McpTokenRecord>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;

        let mut statement = connection.prepare(
            "
            SELECT id, name, subject, created_at, last_used_at, expires_at
            FROM mcp_tokens
            WHERE (?1 IS NULL OR subject = ?1)
            ORDER BY created_at DESC
            ",
        )?;
        let rows = statement.query_map(params![subject], map_mcp_token_row)?;
        let mut tokens = Vec::new();
        for row in rows {
            tokens.push(row?);
        }
        Ok(tokens)
    }

    fn delete_mcp_token(&self, id: &str, subject: Option<&str>) -> Result<bool> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let affected = connection.execute(
            "DELETE FROM mcp_tokens WHERE id = ?1 AND (?2 IS NULL OR subject = ?2)",
            params![id, subject],
        )?;
        Ok(affected > 0)
    }

    fn create_device_auth(&self, request: NewDeviceAuth) -> Result<DeviceAuthRecord> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;

        connection.execute(
            "
            INSERT INTO device_auth_requests
                (device_code, user_code, status, client_name, created_at, expires_at, interval_secs)
            VALUES (?1, ?2, 'pending', ?3, ?4, ?5, ?6)
            ",
            params![
                request.device_code,
                request.user_code,
                request.client_name,
                request.created_at,
                request.expires_at,
                request.interval_secs,
            ],
        )?;

        Ok(DeviceAuthRecord {
            device_code: request.device_code,
            user_code: request.user_code,
            status: DeviceAuthStatus::Pending,
            token_id: None,
            subject: None,
            client_name: request.client_name,
            created_at: request.created_at,
            expires_at: request.expires_at,
            interval_secs: request.interval_secs,
            last_polled_at: None,
        })
    }

    fn find_device_auth_by_device_code(
        &self,
        device_code: &str,
    ) -> Result<Option<DeviceAuthRecord>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let mut statement = connection.prepare(
            "
            SELECT device_code, user_code, status, token_id, subject, client_name,
                   created_at, expires_at, interval_secs, last_polled_at
            FROM device_auth_requests
            WHERE device_code = ?1
            ",
        )?;
        let record = statement
            .query_row(params![device_code], map_device_auth_row)
            .optional()?;
        Ok(record)
    }

    fn find_device_auth_by_user_code(&self, user_code: &str) -> Result<Option<DeviceAuthRecord>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let mut statement = connection.prepare(
            "
            SELECT device_code, user_code, status, token_id, subject, client_name,
                   created_at, expires_at, interval_secs, last_polled_at
            FROM device_auth_requests
            WHERE user_code = ?1
            ",
        )?;
        let record = statement
            .query_row(params![user_code], map_device_auth_row)
            .optional()?;
        Ok(record)
    }

    fn approve_device_auth(
        &self,
        user_code: &str,
        token_id: &str,
        subject: Option<&str>,
        now: i64,
    ) -> Result<bool> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let affected = connection.execute(
            "
            UPDATE device_auth_requests
            SET status = 'approved', token_id = ?1, subject = ?2
            WHERE user_code = ?3 AND status = 'pending' AND expires_at > ?4
            ",
            params![token_id, subject, user_code, now],
        )?;
        Ok(affected > 0)
    }

    fn touch_device_poll(&self, device_code: &str, now: i64) -> Result<()> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        connection.execute(
            "UPDATE device_auth_requests SET last_polled_at = ?1 WHERE device_code = ?2",
            params![now, device_code],
        )?;
        Ok(())
    }

    fn expire_device_auths(&self, now: i64) -> Result<usize> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let affected = connection.execute(
            "
            UPDATE device_auth_requests
            SET status = 'expired'
            WHERE status = 'pending' AND expires_at <= ?1
            ",
            params![now],
        )?;
        Ok(affected)
    }

    fn create_auth_code(&self, code: NewOAuthAuthCode) -> Result<OAuthAuthCodeRecord> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        connection.execute(
            "
            INSERT INTO oauth_authorization_codes
                (code, client_id, redirect_uri, code_challenge, challenge_method,
                 scope, subject, created_at, expires_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ",
            params![
                code.code,
                code.client_id,
                code.redirect_uri,
                code.code_challenge,
                code.challenge_method,
                code.scope,
                code.subject,
                code.created_at,
                code.expires_at,
            ],
        )?;
        Ok(OAuthAuthCodeRecord {
            code: code.code,
            client_id: code.client_id,
            redirect_uri: code.redirect_uri,
            code_challenge: code.code_challenge,
            challenge_method: code.challenge_method,
            scope: code.scope,
            subject: code.subject,
            token_id: None,
            created_at: code.created_at,
            expires_at: code.expires_at,
            consumed_at: None,
        })
    }

    fn find_auth_code(&self, code: &str) -> Result<Option<OAuthAuthCodeRecord>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let mut statement = connection.prepare(
            "
            SELECT code, client_id, redirect_uri, code_challenge, challenge_method,
                   scope, subject, token_id, created_at, expires_at, consumed_at
            FROM oauth_authorization_codes
            WHERE code = ?1
            ",
        )?;
        let record = statement
            .query_row(params![code], map_auth_code_row)
            .optional()?;
        Ok(record)
    }

    fn consume_auth_code(&self, code: &str, token_id: &str, now: i64) -> Result<bool> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let affected = connection.execute(
            "
            UPDATE oauth_authorization_codes
            SET consumed_at = ?1, token_id = ?2
            WHERE code = ?3 AND consumed_at IS NULL AND expires_at > ?1
            ",
            params![now, token_id, code],
        )?;
        Ok(affected > 0)
    }

    fn expire_auth_codes(&self, now: i64) -> Result<usize> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let affected = connection.execute(
            "DELETE FROM oauth_authorization_codes WHERE expires_at <= ?1",
            params![now],
        )?;
        Ok(affected)
    }
}

fn map_mcp_token_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<McpTokenRecord> {
    Ok(McpTokenRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        subject: row.get(2)?,
        created_at: row.get(3)?,
        last_used_at: row.get(4)?,
        expires_at: row.get(5)?,
    })
}

fn map_auth_code_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<OAuthAuthCodeRecord> {
    Ok(OAuthAuthCodeRecord {
        code: row.get(0)?,
        client_id: row.get(1)?,
        redirect_uri: row.get(2)?,
        code_challenge: row.get(3)?,
        challenge_method: row.get(4)?,
        scope: row.get(5)?,
        subject: row.get(6)?,
        token_id: row.get(7)?,
        created_at: row.get(8)?,
        expires_at: row.get(9)?,
        consumed_at: row.get(10)?,
    })
}

fn map_device_auth_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DeviceAuthRecord> {
    let status: String = row.get(2)?;
    let status = DeviceAuthStatus::from_str(&status).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, error.into())
    })?;
    Ok(DeviceAuthRecord {
        device_code: row.get(0)?,
        user_code: row.get(1)?,
        status,
        token_id: row.get(3)?,
        subject: row.get(4)?,
        client_name: row.get(5)?,
        created_at: row.get(6)?,
        expires_at: row.get(7)?,
        interval_secs: row.get(8)?,
        last_polled_at: row.get(9)?,
    })
}
