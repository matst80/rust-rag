//! Third-party integrations (Google, future: Slack/Notion/etc).
//!
//! Each provider lives in its own submodule and exposes route handlers that
//! the top-level `api::router` mounts under `/api/integrations/<provider>/*`.
//! All endpoints require the existing session/api-key auth (the parent
//! `require_api_key` middleware) and operate on the authenticated subject.

pub(super) mod google;
