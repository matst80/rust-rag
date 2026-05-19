-- Web Push (RFC 8030) subscriptions, one row per device/browser the user
-- has authorized. Endpoint + p256dh + auth come from the browser's
-- `pushSubscription.toJSON()`. We store them in cleartext (they're
-- meaningless without the user's private key — the endpoint URL is
-- effectively public).
--
-- (subject, endpoint) is unique so re-subscribing from the same browser
-- upserts rather than duplicating.

CREATE TABLE push_subscriptions (
    id            TEXT PRIMARY KEY,
    subject       TEXT NOT NULL,
    endpoint      TEXT NOT NULL,
    p256dh        TEXT NOT NULL,
    auth          TEXT NOT NULL,
    user_agent    TEXT,
    created_at    BIGINT NOT NULL,
    last_used_at  BIGINT,
    UNIQUE (subject, endpoint)
);

CREATE INDEX idx_push_subscriptions_subject ON push_subscriptions (subject);
