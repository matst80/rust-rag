-- Per-user encrypted OAuth credentials for third-party integrations
-- (Google: Gmail/Calendar/Drive; future: others). Tokens are stored
-- already-encrypted by the caller using the OAUTH_TOKEN_ENC_KEY master
-- key (AES-256-GCM). PK is (subject, provider) so each user has at most
-- one row per provider; reconnecting upserts.

CREATE TABLE user_oauth_credentials (
    subject            TEXT NOT NULL,
    provider           TEXT NOT NULL,
    access_token_enc   TEXT,
    refresh_token_enc  TEXT,
    scopes             TEXT NOT NULL DEFAULT '',
    expires_at         BIGINT,
    account_email      TEXT,
    created_at         BIGINT NOT NULL,
    updated_at         BIGINT NOT NULL,
    PRIMARY KEY (subject, provider)
);

CREATE INDEX idx_user_oauth_subject ON user_oauth_credentials (subject);
