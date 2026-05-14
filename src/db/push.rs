use anyhow::{Context, Result};
use rusqlite::params;
use uuid::Uuid;

use super::{PushStore, PushSubscriptionRecord, SqliteVectorStore, UpsertPushSubscription};

impl PushStore for SqliteVectorStore {
    fn upsert_push_subscription(
        &self,
        sub: UpsertPushSubscription,
    ) -> Result<PushSubscriptionRecord> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;

        let id = Uuid::now_v7().to_string();
        connection.execute(
            "
            INSERT INTO push_subscriptions
                (id, subject, endpoint, p256dh, auth, user_agent, created_at, last_used_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)
            ON CONFLICT(subject, endpoint) DO UPDATE SET
                p256dh = excluded.p256dh,
                auth = excluded.auth,
                user_agent = COALESCE(excluded.user_agent, push_subscriptions.user_agent)
            ",
            params![
                id,
                sub.subject,
                sub.endpoint,
                sub.p256dh,
                sub.auth,
                sub.user_agent,
                sub.now,
            ],
        )?;

        let mut statement = connection.prepare(
            "SELECT id, subject, endpoint, p256dh, auth, user_agent, created_at, last_used_at
             FROM push_subscriptions
             WHERE subject = ?1 AND endpoint = ?2",
        )?;
        let record = statement
            .query_row(params![sub.subject, sub.endpoint], map_row)
            .context("freshly-upserted push subscription missing")?;
        Ok(record)
    }

    fn list_push_subscriptions(&self, subject: &str) -> Result<Vec<PushSubscriptionRecord>> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let mut statement = connection.prepare(
            "SELECT id, subject, endpoint, p256dh, auth, user_agent, created_at, last_used_at
             FROM push_subscriptions
             WHERE subject = ?1
             ORDER BY created_at DESC",
        )?;
        let rows = statement.query_map(params![subject], map_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    fn delete_push_subscription(&self, id: &str, subject: &str) -> Result<bool> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let n = connection.execute(
            "DELETE FROM push_subscriptions WHERE id = ?1 AND subject = ?2",
            params![id, subject],
        )?;
        Ok(n > 0)
    }

    fn delete_push_subscription_by_endpoint(&self, endpoint: &str) -> Result<bool> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        let n = connection.execute(
            "DELETE FROM push_subscriptions WHERE endpoint = ?1",
            params![endpoint],
        )?;
        Ok(n > 0)
    }

    fn touch_push_subscription(&self, id: &str, now: i64) -> Result<()> {
        let guard = self.connection.lock().expect("sqlite mutex poisoned");
        let connection = guard
            .as_ref()
            .context("sqlite connection has already been closed")?;
        connection.execute(
            "UPDATE push_subscriptions SET last_used_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
        Ok(())
    }
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PushSubscriptionRecord> {
    Ok(PushSubscriptionRecord {
        id: row.get(0)?,
        subject: row.get(1)?,
        endpoint: row.get(2)?,
        p256dh: row.get(3)?,
        auth: row.get(4)?,
        user_agent: row.get(5)?,
        created_at: row.get(6)?,
        last_used_at: row.get(7)?,
    })
}

