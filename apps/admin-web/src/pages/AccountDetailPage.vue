<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import { apiRequest } from '../lib/api'
import {
  quotaFillWidth,
  quotaFor,
  quotaMetaText,
  quotaTone,
  remainingPercentText,
} from '../lib/quota'
import type { AccountDetail, AccountOverview, QuotaSnapshot } from '../types'

const route = useRoute()
const router = useRouter()
const loading = ref(true)
const busyAction = ref('')
const error = ref('')
const message = ref('')
const detail = ref<AccountDetail | null>(null)

const accountId = computed(() => route.params.accountId as string)
const fiveHourQuota = computed(() => quotaFor(detail.value?.overview.quotas ?? [], '5h'))
const sevenDayQuota = computed(() => quotaFor(detail.value?.overview.quotas ?? [], '7d'))

function formatTimestamp(value: string | null) {
  if (!value) {
    return '暂无'
  }

  return new Intl.DateTimeFormat('zh-CN', {
    dateStyle: 'short',
    timeStyle: 'short',
  }).format(new Date(value))
}

function formatRemainingTime(value: string | null) {
  if (!value) {
    return '未冷却'
  }

  const diffMs = new Date(value).getTime() - Date.now()
  if (Number.isNaN(diffMs) || diffMs <= 0) {
    return '已到期'
  }

  const totalMinutes = Math.ceil(diffMs / 60000)
  if (totalMinutes < 60) {
    return `${totalMinutes} 分钟`
  }

  const totalHours = Math.floor(totalMinutes / 60)
  const minutes = totalMinutes % 60
  if (totalHours < 48) {
    return minutes > 0 ? `${totalHours} 小时 ${minutes} 分钟` : `${totalHours} 小时`
  }

  const days = Math.floor(totalHours / 24)
  const hours = totalHours % 24
  return hours > 0 ? `${days} 天 ${hours} 小时` : `${days} 天`
}

function formatNumber(value: number) {
  return new Intl.NumberFormat('zh-CN').format(value)
}

function formatSuccessRate(value: number) {
  return `${value.toFixed(value >= 100 || Number.isInteger(value) ? 0 : 1)}%`
}

function quotaDescription(quota: QuotaSnapshot | undefined) {
  return quotaMetaText(quota, formatTimestamp)
}

async function loadDetail(options: { keepMessage?: boolean } = {}) {
  loading.value = true
  error.value = ''
  if (!options.keepMessage) {
    message.value = ''
  }

  try {
    detail.value = await apiRequest<AccountDetail>(`/api/admin/accounts/${accountId.value}`, {
      method: 'GET',
    })
  } catch (err) {
    error.value = err instanceof Error ? err.message : '账号详情加载失败'
  } finally {
    loading.value = false
  }
}

async function patchAccount(payload: Record<string, unknown>) {
  busyAction.value = 'patch'
  error.value = ''
  message.value = ''

  try {
    const overview = await apiRequest<AccountOverview>(`/api/admin/accounts/${accountId.value}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    })

    if (detail.value) {
      detail.value = {
        ...detail.value,
        overview,
      }
    }
    message.value = '账号状态已更新。'
  } catch (err) {
    error.value = err instanceof Error ? err.message : '更新账号失败'
  } finally {
    busyAction.value = ''
  }
}

async function clearCooldown() {
  busyAction.value = 'cooldown'
  error.value = ''
  message.value = ''

  try {
    const overview = await apiRequest<AccountOverview>(
      `/api/admin/accounts/${accountId.value}/clear-cooldown`,
      { method: 'POST' },
    )

    if (detail.value) {
      detail.value = {
        ...detail.value,
        overview,
      }
    }
    message.value = 'Cooldown 已清除。'
  } catch (err) {
    error.value = err instanceof Error ? err.message : '清理 cooldown 失败'
  } finally {
    busyAction.value = ''
  }
}

async function deleteCurrentAccount() {
  if (!detail.value) {
    return
  }

  const confirmed = window.confirm(
    `确认删除账号 "${detail.value.overview.account.name}" 吗？此操作会同时删除配额快照、会话粘性和 usage 记录。`,
  )
  if (!confirmed) {
    return
  }

  busyAction.value = 'delete'
  error.value = ''
  message.value = ''

  try {
    await apiRequest<void>(`/api/admin/accounts/${accountId.value}`, {
      method: 'DELETE',
    })
    await router.push('/accounts')
  } catch (err) {
    error.value = err instanceof Error ? err.message : '删除账号失败'
  } finally {
    busyAction.value = ''
  }
}

async function runAction(path: 'refresh-token' | 'probe-quota') {
  busyAction.value = path
  error.value = ''
  if (path !== 'probe-quota') {
    message.value = ''
  }

  try {
    const result = await apiRequest<{ message: string }>(
      `/api/admin/accounts/${accountId.value}/${path}`,
      { method: 'POST' },
    )
    await loadDetail({ keepMessage: true })

    if (path === 'refresh-token') {
      message.value = result.message
    } else {
      message.value = ''
    }
  } catch (err) {
    error.value = err instanceof Error ? err.message : '动作执行失败'
  } finally {
    busyAction.value = ''
  }
}

onMounted(() => loadDetail())
</script>

<template>
  <section class="page-section stack-lg">
    <header class="page-header">
      <div>
        <p class="eyebrow">Account</p>
        <h2>{{ detail?.overview.account.name ?? '账号详情' }}</h2>
      </div>
      <button class="ghost-button" type="button" @click="loadDetail()">刷新</button>
    </header>

    <div
      v-if="error || message || loading"
      class="status-stack"
      aria-live="polite"
      aria-atomic="true"
    >
      <p v-if="error" class="error-text" role="alert">{{ error }}</p>
      <p v-else-if="message" class="muted" role="status">{{ message }}</p>
      <p v-else-if="loading" class="muted" role="status">正在加载账号详情...</p>
    </div>

    <template v-if="detail && !error && !loading">
      <div class="card-grid two-column-grid">
        <article class="panel-card stack">
          <div class="panel-header">
            <h3>账号状态</h3>
            <span class="status-pill" :data-status="detail.overview.account.status">
              {{ detail.overview.account.status }}
            </span>
          </div>

          <div class="stat-row">
            <span>当前会话</span>
            <div class="stat-value-block">
              <strong>
                {{ detail.overview.current_session_count }} / {{ detail.overview.account.max_sessions }}
              </strong>
              <div class="table-subtext">只统计活跃 WS 会话，不统计已完成的 HTTP 请求。</div>
            </div>
          </div>
          <div class="stat-row">
            <span>Cooldown</span>
            <div class="stat-value-block">
              <strong>
                {{
                  detail.overview.account.cooldown_until
                    ? `${formatTimestamp(detail.overview.account.cooldown_until)} · 剩余 ${formatRemainingTime(detail.overview.account.cooldown_until)}`
                    : '未冷却'
                }}
              </strong>
            </div>
          </div>
          <div class="stat-row">
            <span>最近成功时间</span>
            <strong>{{ formatTimestamp(detail.overview.account.last_success_at) }}</strong>
          </div>
          <div class="stat-row">
            <span>请求成功率</span>
            <div class="stat-value-block">
              <strong>{{ formatSuccessRate(detail.overview.usage.success_rate) }}</strong>
              <div class="table-subtext">
                成功 {{ formatNumber(detail.overview.usage.successful_requests) }} / 总请求
                {{ formatNumber(detail.overview.usage.total_requests) }}
              </div>
            </div>
          </div>
          <div class="stat-row">
            <span>最近错误</span>
            <strong class="wrap-text">{{ detail.overview.account.last_error ?? '暂无' }}</strong>
          </div>
          <div class="stat-row">
            <span>Access Token 过期时间</span>
            <strong>{{ formatTimestamp(detail.secret_metadata.token_expires_at) }}</strong>
          </div>

          <div class="button-row">
            <button
              class="ghost-button"
              type="button"
              :disabled="busyAction !== ''"
              @click="
                patchAccount({
                  status: detail.overview.account.status === 'disabled' ? 'active' : 'disabled',
                })
              "
            >
              {{ detail.overview.account.status === 'disabled' ? '启用账号' : '停用账号' }}
            </button>

            <button
              class="ghost-button"
              type="button"
              :disabled="busyAction !== ''"
              @click="clearCooldown"
            >
              {{ busyAction === 'cooldown' ? '处理中...' : '清除 cooldown' }}
            </button>
            <button
              class="ghost-button danger-button"
              type="button"
              :disabled="busyAction !== ''"
              @click="deleteCurrentAccount"
            >
              {{ busyAction === 'delete' ? '删除中...' : '删除账号' }}
            </button>
          </div>
        </article>

        <article class="panel-card stack">
          <div class="panel-header">
            <h3>配额与兼容性</h3>
            <span class="pill">Live Data</span>
          </div>

          <div class="quota-card">
            <div class="quota-card-head">
              <span>5h 配额</span>
              <strong>{{ remainingPercentText(fiveHourQuota) }}</strong>
            </div>
            <div class="quota-bar">
              <div
                class="quota-bar-fill"
                :data-tone="quotaTone(fiveHourQuota)"
                :style="{ width: quotaFillWidth(fiveHourQuota) }"
              />
            </div>
            <div class="table-subtext">{{ quotaDescription(fiveHourQuota) }}</div>
          </div>

          <div class="quota-card">
            <div class="quota-card-head">
              <span>7d 配额</span>
              <strong>{{ remainingPercentText(sevenDayQuota) }}</strong>
            </div>
            <div class="quota-bar">
              <div
                class="quota-bar-fill"
                :data-tone="quotaTone(sevenDayQuota)"
                :style="{ width: quotaFillWidth(sevenDayQuota) }"
              />
            </div>
            <div class="table-subtext">{{ quotaDescription(sevenDayQuota) }}</div>
          </div>

          <div class="stat-row">
            <span>Fingerprint</span>
            <div class="stat-value-block">
              <strong class="wrap-text">{{ detail.secret_metadata.fingerprint ?? '未设置' }}</strong>
              <div class="table-subtext">当前运行时未使用，仅作为兼容预留字段。</div>
            </div>
          </div>
          <div class="stat-row">
            <span>User-Agent</span>
            <div class="stat-value-block">
              <strong class="wrap-text">{{ detail.secret_metadata.user_agent ?? '未设置' }}</strong>
              <div class="table-subtext">仅在客户端没有传 User-Agent 时，作为回退值使用。</div>
            </div>
          </div>

          <div class="button-row">
            <button
              class="ghost-button"
              type="button"
              :disabled="busyAction !== ''"
              @click="runAction('refresh-token')"
            >
              {{ busyAction === 'refresh-token' ? '处理中...' : '手动刷新 Token' }}
            </button>
            <button
              class="ghost-button"
              type="button"
              :disabled="busyAction !== ''"
              @click="runAction('probe-quota')"
            >
              {{ busyAction === 'probe-quota' ? '处理中...' : '手动 Probe 配额' }}
            </button>
          </div>
        </article>
      </div>

      <article class="panel-card stack">
        <div class="panel-header">
          <h3>近期 Token 用量</h3>
          <span class="pill muted-pill">{{ formatNumber(detail.usage.total_requests) }} requests</span>
        </div>

        <div class="card-grid usage-summary-grid">
          <div class="metric-card">
            <span class="metric-label">成功率</span>
            <strong>{{ formatSuccessRate(detail.usage.success_rate) }}</strong>
          </div>
          <div class="metric-card">
            <span class="metric-label">总请求</span>
            <strong>{{ formatNumber(detail.usage.total_requests) }}</strong>
          </div>
          <div class="metric-card">
            <span class="metric-label">总输入</span>
            <strong>{{ formatNumber(detail.usage.total_input_tokens) }}</strong>
          </div>
          <div class="metric-card">
            <span class="metric-label">总输出</span>
            <strong>{{ formatNumber(detail.usage.total_output_tokens) }}</strong>
          </div>
          <div class="metric-card">
            <span class="metric-label">近 24h 输入</span>
            <strong>{{ formatNumber(detail.usage.input_tokens_last_24h) }}</strong>
          </div>
          <div class="metric-card">
            <span class="metric-label">近 24h 输出</span>
            <strong>{{ formatNumber(detail.usage.output_tokens_last_24h) }}</strong>
          </div>
        </div>

        <div class="table-wrap">
          <table class="data-table">
            <thead>
              <tr>
                <th>时间</th>
                <th>Transport</th>
                <th>Model</th>
                <th>结果</th>
                <th>Input</th>
                <th>Output</th>
                <th>Source</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="event in detail.usage.recent_events" :key="event.id">
                <td>{{ formatTimestamp(event.created_at) }}</td>
                <td>{{ event.transport }}</td>
                <td class="wrap-text">{{ event.model ?? '未知' }}</td>
                <td>{{ event.outcome }}</td>
                <td>{{ event.input_tokens }}</td>
                <td>{{ event.output_tokens }}</td>
                <td class="wrap-text">{{ event.usage_source }}</td>
              </tr>
              <tr v-if="detail.usage.recent_events.length === 0">
                <td colspan="7" class="muted">当前还没有 usage 事件。</td>
              </tr>
            </tbody>
          </table>
        </div>
      </article>
    </template>
  </section>
</template>
