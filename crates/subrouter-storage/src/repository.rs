use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use subrouter_domain::account::DomainValidationError;
use subrouter_domain::{
    Account, AccountDetail, AccountOverview, AccountSecret, AccountSecretMetadata, AccountStatus,
    AccountUsageStats, AccountUsageSummary, CreateAccountInput, DashboardSummary, QuotaSnapshot,
    QuotaSource, RequestOutcome, SessionAffinity, Transport, UpdateAccountInput, UsageEvent,
    UsageSource, WindowType, success_rate_percent,
};
use thiserror::Error;
use uuid::Uuid;

use crate::crypto::{CryptoError, SecretCipher};

#[derive(Clone)]
pub struct Storage {
    pool: PgPool,
    cipher: SecretCipher,
}

impl Storage {
    pub fn new(pool: PgPool, cipher: SecretCipher) -> Self {
        Self { pool, cipher }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn list_accounts(&self) -> Result<Vec<AccountOverview>, StorageError> {
        let accounts = sqlx::query_as::<_, AccountRow>(
            r#"
            SELECT id, name, status, weight, max_sessions, cooldown_until,
                   last_selected_at, last_error, last_success_at, total_requests,
                   successful_requests, failed_requests, cancelled_requests, created_at, updated_at
            FROM accounts
            ORDER BY created_at ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        self.build_account_overviews(accounts).await
    }

    pub async fn create_account(
        &self,
        input: CreateAccountInput,
    ) -> Result<AccountOverview, StorageError> {
        input.validate()?;

        let mut transaction = self.pool.begin().await?;
        let account = sqlx::query_as::<_, AccountRow>(
            r#"
            INSERT INTO accounts (name, status, weight, max_sessions)
            VALUES ($1, 'active', $2, $3)
            RETURNING id, name, status, weight, max_sessions, cooldown_until,
                      last_selected_at, last_error, last_success_at, total_requests,
                      successful_requests, failed_requests, cancelled_requests, created_at, updated_at
            "#,
        )
        .bind(input.name.trim())
        .bind(input.weight)
        .bind(input.max_sessions)
        .fetch_one(&mut *transaction)
        .await
        .map_err(map_database_error)?;

        let secret = AccountSecret {
            account_id: account.id,
            access_token: input.access_token,
            refresh_token: input.refresh_token,
            token_expires_at: input.token_expires_at,
            fingerprint: input.fingerprint,
            user_agent: input.user_agent,
            metadata: input.metadata,
        };
        self.insert_or_replace_secret(transaction.as_mut(), &secret)
            .await?;
        transaction.commit().await?;

        self.get_account_overview(account.id)
            .await?
            .ok_or(StorageError::NotFound("account"))
    }

    pub async fn get_account_detail(
        &self,
        account_id: Uuid,
    ) -> Result<Option<AccountDetail>, StorageError> {
        let account = sqlx::query_as::<_, AccountRow>(
            r#"
            SELECT id, name, status, weight, max_sessions, cooldown_until,
                   last_selected_at, last_error, last_success_at, total_requests,
                   successful_requests, failed_requests, cancelled_requests, created_at, updated_at
            FROM accounts
            WHERE id = $1
            "#,
        )
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(account) = account else {
            return Ok(None);
        };

        let quotas = self.load_quotas_for_account(account_id).await?;
        let current_session_count = self.load_current_session_count(account_id).await?;
        let usage = self.account_usage_summary(account_id).await?;
        let secret_metadata =
            self.load_secret_metadata(account_id)
                .await?
                .unwrap_or(AccountSecretMetadata {
                    token_expires_at: None,
                    fingerprint: None,
                    user_agent: None,
                    metadata: Value::Object(Default::default()),
                });

        Ok(Some(AccountDetail {
            overview: AccountOverview {
                account: account.try_into()?,
                quotas,
                current_session_count,
                usage: usage_stats_from_summary(&usage),
            },
            usage,
            secret_metadata,
        }))
    }

    pub async fn get_account_secret(
        &self,
        account_id: Uuid,
    ) -> Result<Option<AccountSecret>, StorageError> {
        let row = sqlx::query_as::<_, SecretRow>(
            r#"
            SELECT account_id, access_token, refresh_token, token_expires_at,
                   fingerprint, user_agent, metadata
            FROM account_secrets
            WHERE account_id = $1
            "#,
        )
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| self.account_secret_from_row(row)).transpose()
    }

    pub async fn replace_account_secret(&self, secret: &AccountSecret) -> Result<(), StorageError> {
        let mut connection = self.pool.acquire().await?;
        self.insert_or_replace_secret(&mut connection, secret).await
    }

    pub async fn get_account_overview(
        &self,
        account_id: Uuid,
    ) -> Result<Option<AccountOverview>, StorageError> {
        let account = sqlx::query_as::<_, AccountRow>(
            r#"
            SELECT id, name, status, weight, max_sessions, cooldown_until,
                   last_selected_at, last_error, last_success_at, total_requests,
                   successful_requests, failed_requests, cancelled_requests, created_at, updated_at
            FROM accounts
            WHERE id = $1
            "#,
        )
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(account) = account else {
            return Ok(None);
        };

        let quotas = self.load_quotas_for_account(account_id).await?;
        let current_session_count = self.load_current_session_count(account_id).await?;
        let usage = usage_stats_from_account_row(&account);

        Ok(Some(AccountOverview {
            account: account.try_into()?,
            quotas,
            current_session_count,
            usage,
        }))
    }

    pub async fn update_account(
        &self,
        account_id: Uuid,
        input: UpdateAccountInput,
    ) -> Result<Option<AccountOverview>, StorageError> {
        input.validate()?;

        let account = sqlx::query_as::<_, AccountRow>(
            r#"
            UPDATE accounts
            SET name = COALESCE($1, name),
                status = COALESCE($2, status),
                weight = COALESCE($3, weight),
                max_sessions = COALESCE($4, max_sessions),
                last_error = COALESCE($5, last_error),
                updated_at = NOW()
            WHERE id = $6
            RETURNING id, name, status, weight, max_sessions, cooldown_until,
                      last_selected_at, last_error, last_success_at, total_requests,
                      successful_requests, failed_requests, cancelled_requests, created_at, updated_at
            "#,
        )
        .bind(input.name.map(|value| value.trim().to_string()))
        .bind(input.status.map(AccountStatus::as_str))
        .bind(input.weight)
        .bind(input.max_sessions)
        .bind(input.last_error)
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_database_error)?;

        let Some(account) = account else {
            return Ok(None);
        };

        let quotas = self.load_quotas_for_account(account.id).await?;
        let current_session_count = self.load_current_session_count(account.id).await?;
        let usage = usage_stats_from_account_row(&account);

        Ok(Some(AccountOverview {
            account: account.try_into()?,
            quotas,
            current_session_count,
            usage,
        }))
    }

    pub async fn delete_account(&self, account_id: Uuid) -> Result<bool, StorageError> {
        let result = sqlx::query(
            r#"
            DELETE FROM accounts
            WHERE id = $1
            "#,
        )
        .bind(account_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn clear_cooldown(
        &self,
        account_id: Uuid,
    ) -> Result<Option<AccountOverview>, StorageError> {
        let account = sqlx::query_as::<_, AccountRow>(
            r#"
            UPDATE accounts
            SET cooldown_until = NULL, updated_at = NOW()
            WHERE id = $1
            RETURNING id, name, status, weight, max_sessions, cooldown_until,
                      last_selected_at, last_error, last_success_at, total_requests,
                      successful_requests, failed_requests, cancelled_requests, created_at, updated_at
            "#,
        )
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(account) = account else {
            return Ok(None);
        };

        let quotas = self.load_quotas_for_account(account.id).await?;
        let current_session_count = self.load_current_session_count(account.id).await?;
        let usage = usage_stats_from_account_row(&account);

        Ok(Some(AccountOverview {
            account: account.try_into()?,
            quotas,
            current_session_count,
            usage,
        }))
    }

    pub async fn sync_account_quota_state(
        &self,
        account_id: Uuid,
        cooldown_until: Option<DateTime<Utc>>,
        cooldown_reason: Option<&str>,
    ) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            UPDATE accounts
            SET cooldown_until = $1,
                last_error = CASE
                    WHEN $2 THEN $3
                    WHEN last_error LIKE 'quota window exhausted:%' THEN NULL
                    ELSE last_error
                END,
                updated_at = NOW()
            WHERE id = $4
            "#,
        )
        .bind(cooldown_until)
        .bind(cooldown_reason.is_some())
        .bind(cooldown_reason)
        .bind(account_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_session_affinity(
        &self,
        session_key: &str,
    ) -> Result<Option<SessionAffinity>, StorageError> {
        let row = sqlx::query_as::<_, SessionAffinityRow>(
            r#"
            SELECT session_key, account_id, transport, response_id, created_at, last_seen_at, expires_at
            FROM session_affinity
            WHERE session_key = $1
              AND (expires_at IS NULL OR expires_at > NOW())
            "#,
        )
        .bind(session_key)
        .fetch_optional(&self.pool)
        .await?;

        row.map(TryInto::try_into).transpose()
    }

    pub async fn upsert_session_affinity(
        &self,
        affinity: &SessionAffinity,
    ) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            INSERT INTO session_affinity (
                session_key, account_id, transport, response_id, created_at, last_seen_at, expires_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (session_key)
            DO UPDATE SET
                account_id = EXCLUDED.account_id,
                transport = EXCLUDED.transport,
                response_id = EXCLUDED.response_id,
                last_seen_at = EXCLUDED.last_seen_at,
                expires_at = EXCLUDED.expires_at
            "#,
        )
        .bind(&affinity.session_key)
        .bind(affinity.account_id)
        .bind(affinity.transport.as_str())
        .bind(&affinity.response_id)
        .bind(affinity.created_at)
        .bind(affinity.last_seen_at)
        .bind(affinity.expires_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn release_session_affinity(
        &self,
        session_key: &str,
        response_id: Option<&str>,
        sticky_until: Option<DateTime<Utc>>,
    ) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            UPDATE session_affinity
            SET response_id = COALESCE($1, response_id),
                last_seen_at = NOW(),
                expires_at = $2
            WHERE session_key = $3
            "#,
        )
        .bind(response_id)
        .bind(sticky_until)
        .bind(session_key)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn demote_live_ws_affinities(
        &self,
        sticky_until: DateTime<Utc>,
    ) -> Result<u64, StorageError> {
        let result = sqlx::query(
            r#"
            UPDATE session_affinity
            SET last_seen_at = NOW(),
                expires_at = $1
            WHERE transport = 'ws'
              AND expires_at IS NULL
            "#,
        )
        .bind(sticky_until)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn mark_account_selected(&self, account_id: Uuid) -> Result<(), StorageError> {
        self.update_account_runtime(account_id, None, None, false, true, OutcomeDelta::default())
            .await
    }

    pub async fn mark_account_success(&self, account_id: Uuid) -> Result<(), StorageError> {
        self.update_account_runtime(
            account_id,
            None,
            Some(None),
            true,
            false,
            OutcomeDelta::success(),
        )
        .await
    }

    pub async fn mark_account_error(
        &self,
        account_id: Uuid,
        error_message: &str,
    ) -> Result<(), StorageError> {
        self.update_account_runtime(
            account_id,
            None,
            Some(Some(error_message.to_string())),
            false,
            false,
            OutcomeDelta::failed(),
        )
        .await
    }

    pub async fn mark_account_cancelled(&self, account_id: Uuid) -> Result<(), StorageError> {
        self.update_account_runtime(
            account_id,
            None,
            None,
            false,
            false,
            OutcomeDelta::cancelled(),
        )
        .await
    }

    pub async fn mark_account_refresh_failed(
        &self,
        account_id: Uuid,
        error_message: &str,
    ) -> Result<(), StorageError> {
        self.update_account_runtime(
            account_id,
            Some(AccountStatus::RefreshFailed),
            Some(Some(error_message.to_string())),
            false,
            false,
            OutcomeDelta::default(),
        )
        .await
    }

    pub async fn mark_account_active(&self, account_id: Uuid) -> Result<(), StorageError> {
        self.update_account_runtime(
            account_id,
            Some(AccountStatus::Active),
            Some(None),
            false,
            false,
            OutcomeDelta::default(),
        )
        .await
    }

    pub async fn account_usage_summary(
        &self,
        account_id: Uuid,
    ) -> Result<AccountUsageSummary, StorageError> {
        let account_row = sqlx::query_as::<_, AccountUsageCountersRow>(
            r#"
            SELECT total_requests, successful_requests, failed_requests, cancelled_requests
            FROM accounts
            WHERE id = $1
            "#,
        )
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StorageError::NotFound("account"))?;

        let aggregate = sqlx::query_as::<_, UsageAggregateRow>(
            r#"
            SELECT
                COALESCE(SUM(input_tokens), 0)::BIGINT AS total_input_tokens,
                COALESCE(SUM(output_tokens), 0)::BIGINT AS total_output_tokens,
                COALESCE(COUNT(*) FILTER (WHERE created_at >= NOW() - INTERVAL '24 hours'), 0)::BIGINT AS requests_last_24h,
                COALESCE(SUM(input_tokens) FILTER (WHERE created_at >= NOW() - INTERVAL '24 hours'), 0)::BIGINT AS input_tokens_last_24h,
                COALESCE(SUM(output_tokens) FILTER (WHERE created_at >= NOW() - INTERVAL '24 hours'), 0)::BIGINT AS output_tokens_last_24h
            FROM usage_events
            WHERE account_id = $1
            "#,
        )
        .bind(account_id)
        .fetch_one(&self.pool)
        .await?;

        let recent_events = sqlx::query_as::<_, UsageEventRow>(
            r#"
            SELECT id, account_id, transport, model, input_tokens, output_tokens,
                   usage_source, outcome, latency_ms, response_id, session_key, created_at
            FROM usage_events
            WHERE account_id = $1
            ORDER BY created_at DESC
            LIMIT 20
            "#,
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(AccountUsageSummary {
            account_id,
            total_requests: account_row.total_requests,
            successful_requests: account_row.successful_requests,
            failed_requests: account_row.failed_requests,
            cancelled_requests: account_row.cancelled_requests,
            success_rate: success_rate_percent(
                account_row.successful_requests,
                account_row.total_requests,
            ),
            total_input_tokens: aggregate.total_input_tokens,
            total_output_tokens: aggregate.total_output_tokens,
            requests_last_24h: aggregate.requests_last_24h,
            input_tokens_last_24h: aggregate.input_tokens_last_24h,
            output_tokens_last_24h: aggregate.output_tokens_last_24h,
            recent_events: recent_events
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    pub async fn dashboard_summary(&self) -> Result<DashboardSummary, StorageError> {
        let account_summary = sqlx::query_as::<_, DashboardAccountSummaryRow>(
            r#"
            SELECT
                COALESCE(COUNT(*), 0)::BIGINT AS total_accounts,
                COALESCE(COUNT(*) FILTER (WHERE status = 'active'), 0)::BIGINT AS active_accounts,
                COALESCE(COUNT(*) FILTER (WHERE status = 'disabled'), 0)::BIGINT AS disabled_accounts,
                COALESCE(COUNT(*) FILTER (WHERE status = 'refresh_failed'), 0)::BIGINT AS refresh_failed_accounts,
                COALESCE(COUNT(*) FILTER (WHERE cooldown_until IS NOT NULL AND cooldown_until > NOW()), 0)::BIGINT AS accounts_in_cooldown
            FROM accounts
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let usage_summary = sqlx::query_as::<_, DashboardUsageSummaryRow>(
            r#"
            SELECT
                COALESCE(COUNT(*) FILTER (WHERE created_at >= NOW() - INTERVAL '24 hours'), 0)::BIGINT AS requests_last_24h,
                COALESCE(SUM(input_tokens) FILTER (WHERE created_at >= NOW() - INTERVAL '24 hours'), 0)::BIGINT AS input_tokens_last_24h,
                COALESCE(SUM(output_tokens) FILTER (WHERE created_at >= NOW() - INTERVAL '24 hours'), 0)::BIGINT AS output_tokens_last_24h
            FROM usage_events
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let total_active_sessions = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COALESCE(COUNT(*), 0)::BIGINT
            FROM session_affinity
            WHERE transport = 'ws'
              AND expires_at IS NULL
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let highest_five_hour_usage = sqlx::query_scalar::<_, Option<f32>>(
            r#"
            SELECT MAX(used_percent)
            FROM quota_snapshots
            WHERE window_type = '5h'
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let highest_seven_day_usage = sqlx::query_scalar::<_, Option<f32>>(
            r#"
            SELECT MAX(used_percent)
            FROM quota_snapshots
            WHERE window_type = '7d'
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(DashboardSummary {
            total_accounts: account_summary.total_accounts,
            active_accounts: account_summary.active_accounts,
            disabled_accounts: account_summary.disabled_accounts,
            refresh_failed_accounts: account_summary.refresh_failed_accounts,
            accounts_in_cooldown: account_summary.accounts_in_cooldown,
            total_active_sessions,
            requests_last_24h: usage_summary.requests_last_24h,
            input_tokens_last_24h: usage_summary.input_tokens_last_24h,
            output_tokens_last_24h: usage_summary.output_tokens_last_24h,
            highest_five_hour_usage,
            highest_seven_day_usage,
        })
    }

    pub async fn record_usage_event(&self, event: UsageEvent) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            INSERT INTO usage_events (
                id, account_id, transport, model, input_tokens, output_tokens,
                usage_source, outcome, latency_ms, response_id, session_key, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
        )
        .bind(event.id)
        .bind(event.account_id)
        .bind(event.transport.as_str())
        .bind(event.model)
        .bind(event.input_tokens)
        .bind(event.output_tokens)
        .bind(event.usage_source.as_str())
        .bind(event.outcome.as_str())
        .bind(event.latency_ms)
        .bind(event.response_id)
        .bind(event.session_key)
        .bind(event.created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn upsert_quota_snapshot(&self, snapshot: QuotaSnapshot) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            INSERT INTO quota_snapshots (account_id, window_type, used_percent, reset_at, source, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (account_id, window_type)
            DO UPDATE SET
                used_percent = EXCLUDED.used_percent,
                reset_at = EXCLUDED.reset_at,
                source = EXCLUDED.source,
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(snapshot.account_id)
        .bind(snapshot.window_type.as_str())
        .bind(snapshot.used_percent)
        .bind(snapshot.reset_at)
        .bind(snapshot.source.as_str())
        .bind(snapshot.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn latest_probe_quota_update_at(
        &self,
        account_id: Uuid,
    ) -> Result<Option<DateTime<Utc>>, StorageError> {
        sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
            r#"
            SELECT MAX(updated_at)
            FROM quota_snapshots
            WHERE account_id = $1
              AND source = 'probe'
            "#,
        )
        .bind(account_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    async fn build_account_overviews(
        &self,
        accounts: Vec<AccountRow>,
    ) -> Result<Vec<AccountOverview>, StorageError> {
        let quotas = sqlx::query_as::<_, QuotaSnapshotRow>(
            r#"
            SELECT account_id, window_type, used_percent, reset_at, source, updated_at
            FROM quota_snapshots
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let session_counts = sqlx::query_as::<_, SessionCountRow>(
            r#"
            SELECT account_id, COUNT(*)::BIGINT AS current_session_count
            FROM session_affinity
            WHERE transport = 'ws'
              AND expires_at IS NULL
            GROUP BY account_id
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let usage_rows = sqlx::query_as::<_, AccountUsageAggregateRow>(
            r#"
            SELECT
                account_id,
                COALESCE(SUM(input_tokens), 0)::BIGINT AS total_input_tokens,
                COALESCE(SUM(output_tokens), 0)::BIGINT AS total_output_tokens
            FROM usage_events
            GROUP BY account_id
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let quota_map = quotas.into_iter().try_fold(
            HashMap::<Uuid, Vec<QuotaSnapshot>>::new(),
            |mut acc, row| -> Result<_, StorageError> {
                acc.entry(row.account_id).or_default().push(row.try_into()?);
                Ok(acc)
            },
        )?;

        let session_map = session_counts
            .into_iter()
            .map(|row| (row.account_id, row.current_session_count))
            .collect::<HashMap<_, _>>();

        let usage_map = usage_rows
            .into_iter()
            .map(|row| {
                (
                    row.account_id,
                    AccountUsageStats {
                        total_requests: 0,
                        successful_requests: 0,
                        failed_requests: 0,
                        cancelled_requests: 0,
                        total_input_tokens: row.total_input_tokens,
                        total_output_tokens: row.total_output_tokens,
                        success_rate: 0.0,
                    },
                )
            })
            .collect::<HashMap<_, _>>();

        accounts
            .into_iter()
            .map(|row| {
                let total_requests = row.total_requests;
                let successful_requests = row.successful_requests;
                let failed_requests = row.failed_requests;
                let cancelled_requests = row.cancelled_requests;
                let account: Account = row.try_into()?;
                let mut usage = usage_map.get(&account.id).cloned().unwrap_or_default();
                usage.total_requests = total_requests;
                usage.successful_requests = successful_requests;
                usage.failed_requests = failed_requests;
                usage.cancelled_requests = cancelled_requests;
                usage.success_rate = success_rate_percent(successful_requests, total_requests);
                Ok(AccountOverview {
                    current_session_count: session_map.get(&account.id).copied().unwrap_or(0),
                    quotas: quota_map.get(&account.id).cloned().unwrap_or_default(),
                    usage,
                    account,
                })
            })
            .collect()
    }

    async fn load_quotas_for_account(
        &self,
        account_id: Uuid,
    ) -> Result<Vec<QuotaSnapshot>, StorageError> {
        let rows = sqlx::query_as::<_, QuotaSnapshotRow>(
            r#"
            SELECT account_id, window_type, used_percent, reset_at, source, updated_at
            FROM quota_snapshots
            WHERE account_id = $1
            ORDER BY window_type ASC
            "#,
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn load_current_session_count(&self, account_id: Uuid) -> Result<i64, StorageError> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COALESCE(COUNT(*), 0)::BIGINT
            FROM session_affinity
            WHERE account_id = $1
              AND transport = 'ws'
              AND expires_at IS NULL
            "#,
        )
        .bind(account_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(count)
    }

    async fn load_secret_metadata(
        &self,
        account_id: Uuid,
    ) -> Result<Option<AccountSecretMetadata>, StorageError> {
        let row = sqlx::query_as::<_, SecretMetadataRow>(
            r#"
            SELECT token_expires_at, fingerprint, user_agent, metadata
            FROM account_secrets
            WHERE account_id = $1
            "#,
        )
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| self.secret_metadata_from_row(row))
            .transpose()
    }

    fn secret_metadata_from_row(
        &self,
        row: SecretMetadataRow,
    ) -> Result<AccountSecretMetadata, StorageError> {
        Ok(AccountSecretMetadata {
            token_expires_at: row
                .token_expires_at
                .map(|value| self.cipher.decrypt_string(&value))
                .transpose()?
                .map(|value| parse_timestamp(&value))
                .transpose()?,
            fingerprint: row
                .fingerprint
                .map(|value| self.cipher.decrypt_string(&value))
                .transpose()?,
            user_agent: row
                .user_agent
                .map(|value| self.cipher.decrypt_string(&value))
                .transpose()?,
            metadata: row
                .metadata
                .map(|value| self.cipher.decrypt_json(&value))
                .transpose()?
                .unwrap_or_else(|| Value::Object(Default::default())),
        })
    }

    fn account_secret_from_row(&self, row: SecretRow) -> Result<AccountSecret, StorageError> {
        Ok(AccountSecret {
            account_id: row.account_id,
            access_token: self.cipher.decrypt_string(&row.access_token)?,
            refresh_token: self.cipher.decrypt_string(&row.refresh_token)?,
            token_expires_at: row
                .token_expires_at
                .map(|value| self.cipher.decrypt_string(&value))
                .transpose()?
                .map(|value| parse_timestamp(&value))
                .transpose()?,
            fingerprint: row
                .fingerprint
                .map(|value| self.cipher.decrypt_string(&value))
                .transpose()?,
            user_agent: row
                .user_agent
                .map(|value| self.cipher.decrypt_string(&value))
                .transpose()?,
            metadata: row
                .metadata
                .map(|value| self.cipher.decrypt_json(&value))
                .transpose()?
                .unwrap_or_else(|| Value::Object(Default::default())),
        })
    }

    async fn update_account_runtime(
        &self,
        account_id: Uuid,
        status: Option<AccountStatus>,
        last_error: Option<Option<String>>,
        set_last_success_at: bool,
        set_last_selected_at: bool,
        outcome_delta: OutcomeDelta,
    ) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            UPDATE accounts
            SET status = COALESCE($1, status),
                last_error = CASE
                    WHEN $2 THEN $3
                    ELSE last_error
                END,
                last_success_at = CASE
                    WHEN $4 THEN NOW()
                    ELSE last_success_at
                END,
                last_selected_at = CASE
                    WHEN $5 THEN NOW()
                    ELSE last_selected_at
                END,
                total_requests = total_requests + $6,
                successful_requests = successful_requests + $7,
                failed_requests = failed_requests + $8,
                cancelled_requests = cancelled_requests + $9,
                updated_at = NOW()
            WHERE id = $10
            "#,
        )
        .bind(status.map(AccountStatus::as_str))
        .bind(last_error.is_some())
        .bind(last_error.flatten())
        .bind(set_last_success_at)
        .bind(set_last_selected_at)
        .bind(outcome_delta.total_requests)
        .bind(outcome_delta.successful_requests)
        .bind(outcome_delta.failed_requests)
        .bind(outcome_delta.cancelled_requests)
        .bind(account_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn insert_or_replace_secret(
        &self,
        executor: &mut sqlx::PgConnection,
        secret: &AccountSecret,
    ) -> Result<(), StorageError> {
        let encrypted_access_token = self.cipher.encrypt_string(&secret.access_token)?;
        let encrypted_refresh_token = self.cipher.encrypt_string(&secret.refresh_token)?;
        let encrypted_token_expires_at = secret
            .token_expires_at
            .map(|value| self.cipher.encrypt_string(&value.to_rfc3339()))
            .transpose()?;
        let encrypted_fingerprint = secret
            .fingerprint
            .as_deref()
            .map(|value| self.cipher.encrypt_string(value))
            .transpose()?;
        let encrypted_user_agent = secret
            .user_agent
            .as_deref()
            .map(|value| self.cipher.encrypt_string(value))
            .transpose()?;
        let encrypted_metadata = Some(self.cipher.encrypt_json(&secret.metadata)?);

        sqlx::query(
            r#"
            INSERT INTO account_secrets (
                account_id, access_token, refresh_token, token_expires_at,
                fingerprint, user_agent, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (account_id)
            DO UPDATE SET
                access_token = EXCLUDED.access_token,
                refresh_token = EXCLUDED.refresh_token,
                token_expires_at = EXCLUDED.token_expires_at,
                fingerprint = EXCLUDED.fingerprint,
                user_agent = EXCLUDED.user_agent,
                metadata = EXCLUDED.metadata,
                updated_at = NOW()
            "#,
        )
        .bind(secret.account_id)
        .bind(encrypted_access_token)
        .bind(encrypted_refresh_token)
        .bind(encrypted_token_expires_at)
        .bind(encrypted_fingerprint)
        .bind(encrypted_user_agent)
        .bind(encrypted_metadata)
        .execute(executor)
        .await?;

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("resource not found: {0}")]
    NotFound(&'static str),
    #[error("conflict: {0}")]
    Conflict(String),
}

impl From<DomainValidationError> for StorageError {
    fn from(value: DomainValidationError) -> Self {
        Self::Validation(value.to_string())
    }
}

impl From<chrono::ParseError> for StorageError {
    fn from(value: chrono::ParseError) -> Self {
        Self::Validation(value.to_string())
    }
}

#[derive(Debug, FromRow)]
struct AccountRow {
    id: Uuid,
    name: String,
    status: String,
    weight: i32,
    max_sessions: i32,
    cooldown_until: Option<DateTime<Utc>>,
    last_selected_at: Option<DateTime<Utc>>,
    last_error: Option<String>,
    last_success_at: Option<DateTime<Utc>>,
    total_requests: i64,
    successful_requests: i64,
    failed_requests: i64,
    cancelled_requests: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct QuotaSnapshotRow {
    account_id: Uuid,
    window_type: String,
    used_percent: f32,
    reset_at: Option<DateTime<Utc>>,
    source: String,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct UsageEventRow {
    id: Uuid,
    account_id: Uuid,
    transport: String,
    model: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
    usage_source: String,
    outcome: String,
    latency_ms: Option<i32>,
    response_id: Option<String>,
    session_key: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct UsageAggregateRow {
    total_input_tokens: i64,
    total_output_tokens: i64,
    requests_last_24h: i64,
    input_tokens_last_24h: i64,
    output_tokens_last_24h: i64,
}

#[derive(Debug, FromRow)]
struct AccountUsageCountersRow {
    total_requests: i64,
    successful_requests: i64,
    failed_requests: i64,
    cancelled_requests: i64,
}

#[derive(Debug, FromRow)]
struct AccountUsageAggregateRow {
    account_id: Uuid,
    total_input_tokens: i64,
    total_output_tokens: i64,
}

#[derive(Debug, FromRow)]
struct DashboardAccountSummaryRow {
    total_accounts: i64,
    active_accounts: i64,
    disabled_accounts: i64,
    refresh_failed_accounts: i64,
    accounts_in_cooldown: i64,
}

#[derive(Debug, FromRow)]
struct DashboardUsageSummaryRow {
    requests_last_24h: i64,
    input_tokens_last_24h: i64,
    output_tokens_last_24h: i64,
}

#[derive(Debug, FromRow)]
struct SecretMetadataRow {
    token_expires_at: Option<Vec<u8>>,
    fingerprint: Option<Vec<u8>>,
    user_agent: Option<Vec<u8>>,
    metadata: Option<Vec<u8>>,
}

#[derive(Debug, FromRow)]
struct SecretRow {
    account_id: Uuid,
    access_token: Vec<u8>,
    refresh_token: Vec<u8>,
    token_expires_at: Option<Vec<u8>>,
    fingerprint: Option<Vec<u8>>,
    user_agent: Option<Vec<u8>>,
    metadata: Option<Vec<u8>>,
}

#[derive(Debug, FromRow)]
struct SessionCountRow {
    account_id: Uuid,
    current_session_count: i64,
}

#[derive(Debug, FromRow)]
struct SessionAffinityRow {
    session_key: String,
    account_id: Uuid,
    transport: String,
    response_id: Option<String>,
    created_at: DateTime<Utc>,
    last_seen_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
}

impl TryFrom<AccountRow> for Account {
    type Error = StorageError;

    fn try_from(value: AccountRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            name: value.name,
            status: AccountStatus::try_from(value.status.as_str())?,
            weight: value.weight,
            max_sessions: value.max_sessions,
            cooldown_until: value.cooldown_until,
            last_selected_at: value.last_selected_at,
            last_error: value.last_error,
            last_success_at: value.last_success_at,
            created_at: value.created_at,
            updated_at: value.updated_at,
        })
    }
}

impl TryFrom<QuotaSnapshotRow> for QuotaSnapshot {
    type Error = StorageError;

    fn try_from(value: QuotaSnapshotRow) -> Result<Self, Self::Error> {
        let window_type = match value.window_type.as_str() {
            "5h" => WindowType::FiveHours,
            "7d" => WindowType::SevenDays,
            other => {
                return Err(StorageError::Validation(format!(
                    "unknown quota window `{other}`"
                )));
            }
        };
        let source = match value.source.as_str() {
            "header" => QuotaSource::Header,
            "probe" => QuotaSource::Probe,
            other => {
                return Err(StorageError::Validation(format!(
                    "unknown quota source `{other}`"
                )));
            }
        };

        Ok(Self {
            account_id: value.account_id,
            window_type,
            used_percent: value.used_percent,
            reset_at: value.reset_at,
            source,
            updated_at: value.updated_at,
        })
    }
}

impl TryFrom<UsageEventRow> for UsageEvent {
    type Error = StorageError;

    fn try_from(value: UsageEventRow) -> Result<Self, Self::Error> {
        let transport = match value.transport.as_str() {
            "http" => Transport::Http,
            "ws" => Transport::Ws,
            other => {
                return Err(StorageError::Validation(format!(
                    "unknown transport `{other}`"
                )));
            }
        };
        let usage_source = match value.usage_source.as_str() {
            "exact" => UsageSource::Exact,
            "estimated" => UsageSource::Estimated,
            other => {
                return Err(StorageError::Validation(format!(
                    "unknown usage source `{other}`"
                )));
            }
        };
        let outcome = match value.outcome.as_str() {
            "success" => RequestOutcome::Success,
            "failed" => RequestOutcome::Failed,
            "cancelled" => RequestOutcome::Cancelled,
            other => {
                return Err(StorageError::Validation(format!(
                    "unknown request outcome `{other}`"
                )));
            }
        };

        Ok(Self {
            id: value.id,
            account_id: value.account_id,
            transport,
            model: value.model,
            input_tokens: value.input_tokens,
            output_tokens: value.output_tokens,
            usage_source,
            outcome,
            latency_ms: value.latency_ms,
            response_id: value.response_id,
            session_key: value.session_key,
            created_at: value.created_at,
        })
    }
}

impl TryFrom<SessionAffinityRow> for SessionAffinity {
    type Error = StorageError;

    fn try_from(value: SessionAffinityRow) -> Result<Self, Self::Error> {
        Ok(Self {
            session_key: value.session_key,
            account_id: value.account_id,
            transport: parse_transport(&value.transport)?,
            response_id: value.response_id,
            created_at: value.created_at,
            last_seen_at: value.last_seen_at,
            expires_at: value.expires_at,
        })
    }
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, chrono::ParseError> {
    DateTime::parse_from_rfc3339(value).map(|value| value.with_timezone(&Utc))
}

fn parse_transport(value: &str) -> Result<Transport, StorageError> {
    match value {
        "http" => Ok(Transport::Http),
        "ws" => Ok(Transport::Ws),
        other => Err(StorageError::Validation(format!(
            "unknown transport `{other}`"
        ))),
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct OutcomeDelta {
    total_requests: i64,
    successful_requests: i64,
    failed_requests: i64,
    cancelled_requests: i64,
}

impl OutcomeDelta {
    fn success() -> Self {
        Self {
            total_requests: 1,
            successful_requests: 1,
            ..Self::default()
        }
    }

    fn failed() -> Self {
        Self {
            total_requests: 1,
            failed_requests: 1,
            ..Self::default()
        }
    }

    fn cancelled() -> Self {
        Self {
            total_requests: 1,
            cancelled_requests: 1,
            ..Self::default()
        }
    }
}

fn usage_stats_from_account_row(row: &AccountRow) -> AccountUsageStats {
    AccountUsageStats {
        total_requests: row.total_requests,
        successful_requests: row.successful_requests,
        failed_requests: row.failed_requests,
        cancelled_requests: row.cancelled_requests,
        total_input_tokens: 0,
        total_output_tokens: 0,
        success_rate: success_rate_percent(row.successful_requests, row.total_requests),
    }
}

fn usage_stats_from_summary(summary: &AccountUsageSummary) -> AccountUsageStats {
    AccountUsageStats {
        total_requests: summary.total_requests,
        successful_requests: summary.successful_requests,
        failed_requests: summary.failed_requests,
        cancelled_requests: summary.cancelled_requests,
        total_input_tokens: summary.total_input_tokens,
        total_output_tokens: summary.total_output_tokens,
        success_rate: summary.success_rate,
    }
}

fn map_database_error(error: sqlx::Error) -> StorageError {
    if let sqlx::Error::Database(database_error) = &error {
        if database_error.code().as_deref() == Some("23505") {
            return StorageError::Conflict(database_error.message().to_string());
        }
    }
    StorageError::Database(error)
}
