use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use subrouter_domain::UsageEvent;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageWindowSnapshot {
    pub account_id: Uuid,
    pub collected_at: DateTime<Utc>,
    #[serde(default)]
    pub events: Vec<UsageEvent>,
}
