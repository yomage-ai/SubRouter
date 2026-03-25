export interface Account {
  id: string
  name: string
  status: 'active' | 'disabled' | 'refresh_failed'
  weight: number
  max_sessions: number
  cooldown_until: string | null
  last_selected_at: string | null
  last_error: string | null
  last_success_at: string | null
  created_at: string
  updated_at: string
}

export interface AccountUsageStats {
  total_requests: number
  successful_requests: number
  failed_requests: number
  cancelled_requests: number
  total_input_tokens: number
  total_output_tokens: number
  success_rate: number
}

export interface QuotaSnapshot {
  account_id: string
  window_type: '5h' | '7d'
  used_percent: number
  reset_at: string | null
  source: 'header' | 'probe'
  updated_at: string
}

export interface AccountOverview {
  account: Account
  quotas: QuotaSnapshot[]
  current_session_count: number
  usage: AccountUsageStats
}

export interface UsageEvent {
  id: string
  account_id: string
  transport: 'http' | 'ws'
  model: string | null
  input_tokens: number
  output_tokens: number
  usage_source: 'exact' | 'estimated'
  outcome: 'success' | 'failed' | 'cancelled'
  latency_ms: number | null
  response_id: string | null
  session_key: string | null
  created_at: string
}

export interface AccountUsageSummary {
  account_id: string
  total_requests: number
  successful_requests: number
  failed_requests: number
  cancelled_requests: number
  success_rate: number
  total_input_tokens: number
  total_output_tokens: number
  requests_last_24h: number
  input_tokens_last_24h: number
  output_tokens_last_24h: number
  recent_events: UsageEvent[]
}

export interface AccountSecretMetadata {
  token_expires_at: string | null
  fingerprint: string | null
  user_agent: string | null
  metadata: Record<string, unknown>
}

export interface AccountDetail {
  overview: AccountOverview
  usage: AccountUsageSummary
  secret_metadata: AccountSecretMetadata
}

export interface DashboardSummary {
  total_accounts: number
  active_accounts: number
  disabled_accounts: number
  refresh_failed_accounts: number
  accounts_in_cooldown: number
  total_active_sessions: number
  requests_last_24h: number
  input_tokens_last_24h: number
  output_tokens_last_24h: number
  highest_five_hour_usage: number | null
  highest_seven_day_usage: number | null
}

export type DashboardTokenRange = '24h' | '7d' | '30d'

export interface DashboardTokenUsagePoint {
  bucket_start: string
  request_count: number
  input_tokens: number
  output_tokens: number
  total_tokens: number
}

export interface DashboardTokenUsageSeries {
  range: DashboardTokenRange
  bucket_seconds: number
  total_requests: number
  total_input_tokens: number
  total_output_tokens: number
  points: DashboardTokenUsagePoint[]
}
