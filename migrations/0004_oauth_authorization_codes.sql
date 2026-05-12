-- OAuth 2.1 authorization_code + PKCE flow for spec-compliant MCP HTTP
-- clients (VSCode, Cursor). Coexists with device_auth_requests; both
-- mint into mcp_tokens.

CREATE TABLE oauth_authorization_codes (
    code             TEXT PRIMARY KEY,
    client_id        TEXT NOT NULL,
    redirect_uri     TEXT NOT NULL,
    code_challenge   TEXT NOT NULL,
    challenge_method TEXT NOT NULL CHECK (challenge_method IN ('S256')),
    scope            TEXT,
    subject          TEXT,
    token_id         TEXT REFERENCES mcp_tokens(id) ON DELETE SET NULL,
    created_at       BIGINT NOT NULL,
    expires_at       BIGINT NOT NULL,
    consumed_at      BIGINT
);

CREATE INDEX idx_oauth_codes_expires ON oauth_authorization_codes (expires_at);
