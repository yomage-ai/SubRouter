use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthCredentialRefresh {
    pub account_id: Uuid,
    pub attempted_at: DateTime<Utc>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaProbeResult {
    pub account_id: Uuid,
    pub observed_at: DateTime<Utc>,
    #[serde(default)]
    pub headers: Value,
}
