use anyhow::{Context, Result};
use rusqlite::{OptionalExtension, params};

use super::{
    OAuthCredentialsRecord, OAuthCredsStore, SqliteVectorStore, UpsertOAuthCredentials,
};

impl OAuthCredsStore for SqliteVectorStore {
    fn upsert_oauth_credentials(
        &self,
        creds: UpsertOAuthCredentials,
    ) -> Result<OAuthCredentialsRecord> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;

        // INSERT ... ON CONFLICT: preserve created_at if a row already exists.
        connection.execute(
            "
            INSERT INTO user_oauth_credentials (
                subject, provider, access_token_enc, refresh_token_enc,
                scopes, expires_at, account_email, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
            ON CONFLICT(subject, provider) DO UPDATE SET
                access_token_enc = excluded.access_token_enc,
                refresh_token_enc = COALESCE(excluded.refresh_token_enc, user_oauth_credentials.refresh_token_enc),
                scopes = excluded.scopes,
                expires_at = excluded.expires_at,
                account_email = COALESCE(excluded.account_email, user_oauth_credentials.account_email),
                updated_at = excluded.updated_at
            ",
            params![
                creds.subject,
                creds.provider,
                creds.access_token_enc,
                creds.refresh_token_enc,
                creds.scopes,
                creds.expires_at,
                creds.account_email,
                creds.now,
            ],
        )?;

        let mut statement = connection.prepare(
            "SELECT subject, provider, access_token_enc, refresh_token_enc,
                    scopes, expires_at, account_email, created_at, updated_at
             FROM user_oauth_credentials
             WHERE subject = ?1 AND provider = ?2",
        )?;
        let record = statement
            .query_row(params![creds.subject, creds.provider], map_row)
            .context("freshly-upserted oauth credentials row missing")?;
        Ok(record)
    }

    fn find_oauth_credentials(
        &self,
        subject: &str,
        provider: &str,
    ) -> Result<Option<OAuthCredentialsRecord>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let mut statement = connection.prepare(
            "SELECT subject, provider, access_token_enc, refresh_token_enc,
                    scopes, expires_at, account_email, created_at, updated_at
             FROM user_oauth_credentials
             WHERE subject = ?1 AND provider = ?2",
        )?;
        let row = statement
            .query_row(params![subject, provider], map_row)
            .optional()?;
        Ok(row)
    }

    fn delete_oauth_credentials(&self, subject: &str, provider: &str) -> Result<bool> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let affected = connection.execute(
            "DELETE FROM user_oauth_credentials WHERE subject = ?1 AND provider = ?2",
            params![subject, provider],
        )?;
        Ok(affected > 0)
    }

    fn list_oauth_providers(&self, subject: &str) -> Result<Vec<OAuthCredentialsRecord>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let mut statement = connection.prepare(
            "SELECT subject, provider, access_token_enc, refresh_token_enc,
                    scopes, expires_at, account_email, created_at, updated_at
             FROM user_oauth_credentials
             WHERE subject = ?1
             ORDER BY provider",
        )?;
        let rows = statement.query_map(params![subject], map_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<OAuthCredentialsRecord> {
    Ok(OAuthCredentialsRecord {
        subject: row.get(0)?,
        provider: row.get(1)?,
        access_token_enc: row.get(2)?,
        refresh_token_enc: row.get(3)?,
        scopes: row.get(4)?,
        expires_at: row.get(5)?,
        account_email: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}
