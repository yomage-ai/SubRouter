<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from 'vue'
import { RouterLink } from 'vue-router'

import { apiRequest } from '../lib/api'
import {
  isUrgentSevenDayReset,
  quotaFillWidth,
  quotaFor,
  quotaPriorityScore,
  quotaTone,
  remainingPercentText,
  resetWindowRemainingPercent,
} from '../lib/quota'
import type { AccountOverview, QuotaSnapshot } from '../types'

interface ClipboardImportPayload {
  auth_mode?: string | null
  OPENAI_API_KEY?: string | null
  tokens?: {
    id_token?: string | null
    access_token?: string | null
    refresh_token?: string | null
    account_id?: string | null
  } | null
  last_refresh?: string | null
}

interface CodexLoginStartResponse {
  auth_url: string
  callback_origin: string
}

interface CodexLoginMessage {
  source?: string
  status?: 'success' | 'error'
  message?: string
}

const loading = ref(true)
const saving = ref(false)
const probingAll = ref(false)
const importing = ref(false)
const oauthStarting = ref(false)
const error = ref('')
const message = ref('')
const accounts = ref<AccountOverview[]>([])
const expectedOauthOrigin = ref('')

const form = reactive({
  name: '',
  weight: 1,
  access_token: '',
  refresh_token: '',
  token_expires_at: '',
  fingerprint: '',
  user_agent: '',
})

const hasAccounts = computed(() => accounts.value.length > 0)
const sortedAccounts = computed(() =>
  [...accounts.value].sort((left, right) => compareAccounts(left, right)),
)

async function loadAccounts() {
  loading.value = true
  error.value = ''

  try {
    accounts.value = await apiRequest<AccountOverview[]>('/api/admin/accounts', {
      method: 'GET',
    })
  } catch (err) {
    error.value = err instanceof Error ? err.message : '账号列表加载失败'
  } finally {
    loading.value = false
  }
}

async function probeAllQuotas() {
  probingAll.value = true
  error.value = ''
  message.value = ''

  try {
    const result = await apiRequest<{ message: string }>('/api/admin/accounts/probe-quota', {
      method: 'POST',
    })
    await loadAccounts()
    message.value = result.message
  } catch (err) {
    error.value = err instanceof Error ? err.message : '批量 Probe 配额失败'
  } finally {
    probingAll.value = false
  }
}

async function pasteFromClipboard() {
  if (typeof navigator === 'undefined' || !navigator.clipboard?.readText) {
    error.value = '当前环境不支持读取剪贴板，请手动粘贴 JSON。'
    return
  }

  importing.value = true
  error.value = ''
  message.value = ''

  try {
    const text = await navigator.clipboard.readText()
    const payload = JSON.parse(text) as ClipboardImportPayload
    const tokens = payload.tokens

    if (payload.auth_mode && payload.auth_mode !== 'chatgpt') {
      throw new Error('仅支持导入 auth_mode = chatgpt 的账号 JSON。')
    }

    if (!tokens?.access_token || !tokens?.refresh_token) {
      throw new Error('剪贴板 JSON 缺少 access_token 或 refresh_token。')
    }

    form.access_token = tokens.access_token
    form.refresh_token = tokens.refresh_token

    if (!form.name.trim() && tokens.account_id) {
      form.name = `chatgpt-${tokens.account_id.slice(0, 8)}`
    }

    message.value = tokens.account_id
      ? `已从剪贴板导入账号 ${tokens.account_id.slice(0, 8)} 的 token。`
      : '已从剪贴板导入 token。'
  } catch (err) {
    error.value = err instanceof Error ? err.message : '读取剪贴板失败'
  } finally {
    importing.value = false
  }
}

async function startCodexLogin() {
  oauthStarting.value = true
  error.value = ''
  message.value = ''

  try {
    const response = await apiRequest<CodexLoginStartResponse>('/api/admin/oauth/codex/start', {
      method: 'POST',
      body: JSON.stringify({
        name: form.name || null,
        weight: form.weight,
      }),
    })

    const popup = window.open(
      response.auth_url,
      'subrouter-codex-oauth',
      'popup=yes,width=720,height=840,resizable=yes,scrollbars=yes',
    )
    if (!popup) {
      throw new Error('浏览器拦截了登录窗口，请允许弹窗后重试。')
    }

    expectedOauthOrigin.value = response.callback_origin
    popup.focus()
    message.value = 'Codex 登录窗口已打开，完成登录后会自动回到这里。'
  } catch (err) {
    error.value = err instanceof Error ? err.message : '启动 Codex 登录失败'
  } finally {
    oauthStarting.value = false
  }
}

function formatTimestamp(value: string | null) {
  if (!value) {
    return '未知'
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

function resetForm() {
  form.name = ''
  form.weight = 1
  form.access_token = ''
  form.refresh_token = ''
  form.token_expires_at = ''
  form.fingerprint = ''
  form.user_agent = ''
}

function quotaLabel(quota: QuotaSnapshot | undefined, fallback: string) {
  return quota ? remainingPercentText(quota) : fallback
}

function resetProgressWidth(quota: QuotaSnapshot | undefined) {
  const progress = resetWindowRemainingPercent(quota)
  if (progress === null) {
    return '0%'
  }

  return `${progress}%`
}

function resetProgressTooltip(quota: QuotaSnapshot | undefined) {
  if (!quota?.reset_at) {
    return '等待观测 7 天重置时间'
  }

  return `7 天窗口剩余 ${formatRemainingTime(quota.reset_at)} 后重置`
}

function compareAccounts(left: AccountOverview, right: AccountOverview) {
  const leftSevenDay = quotaFor(left.quotas, '7d')
  const rightSevenDay = quotaFor(right.quotas, '7d')
  const leftFiveHour = quotaFor(left.quotas, '5h')
  const rightFiveHour = quotaFor(right.quotas, '5h')

  const byAvailability = compareAsc(
    isQuotaDepleted(leftSevenDay, leftFiveHour) ? 1 : 0,
    isQuotaDepleted(rightSevenDay, rightFiveHour) ? 1 : 0,
  )
  if (byAvailability !== 0) {
    return byAvailability
  }

  const byUrgentReset = compareAsc(
    isUrgentSevenDayReset(leftSevenDay) ? 0 : 1,
    isUrgentSevenDayReset(rightSevenDay) ? 0 : 1,
  )
  if (byUrgentReset !== 0) {
    return byUrgentReset
  }

  const byPriority = compareDesc(
    quotaPriorityScore(
      leftSevenDay,
      leftFiveHour,
    ),
    quotaPriorityScore(
      rightSevenDay,
      rightFiveHour,
    ),
  )
  if (byPriority !== 0) {
    return byPriority
  }

  const bySessions = left.current_session_count - right.current_session_count
  if (bySessions !== 0) {
    return bySessions
  }

  const byLastSelected = compareOptionalTimestamp(
    left.account.last_selected_at,
    right.account.last_selected_at,
  )
  if (byLastSelected !== 0) {
    return byLastSelected
  }

  return left.account.name.localeCompare(right.account.name, 'zh-CN')
}

function quotaRemainingNumber(quota: QuotaSnapshot | undefined) {
  return quota ? Math.max(0, 100 - quota.used_percent) : null
}

function isQuotaDepleted(
  sevenDayQuota: QuotaSnapshot | undefined,
  fiveHourQuota: QuotaSnapshot | undefined,
) {
  return quotaRemainingNumber(sevenDayQuota) === 0 || quotaRemainingNumber(fiveHourQuota) === 0
}

function compareDesc(left: number | null, right: number | null) {
  const normalizedLeft = left ?? Number.NEGATIVE_INFINITY
  const normalizedRight = right ?? Number.NEGATIVE_INFINITY
  if (normalizedLeft === normalizedRight) {
    return 0
  }

  return normalizedLeft > normalizedRight ? -1 : 1
}

function compareAsc(left: number | null, right: number | null) {
  const normalizedLeft = left ?? Number.POSITIVE_INFINITY
  const normalizedRight = right ?? Number.POSITIVE_INFINITY
  if (normalizedLeft === normalizedRight) {
    return 0
  }

  return normalizedLeft < normalizedRight ? -1 : 1
}

function compareOptionalTimestamp(left: string | null, right: string | null) {
  const normalizedLeft = left ? new Date(left).getTime() : Number.NEGATIVE_INFINITY
  const normalizedRight = right ? new Date(right).getTime() : Number.NEGATIVE_INFINITY
  if (normalizedLeft === normalizedRight) {
    return 0
  }

  return normalizedLeft < normalizedRight ? -1 : 1
}

function latestAccountNote(item: AccountOverview) {
  if (item.account.cooldown_until) {
    return `冷却剩余 ${formatRemainingTime(item.account.cooldown_until)}`
  }

  if (item.account.last_success_at) {
    return `最近成功 ${formatTimestamp(item.account.last_success_at)}`
  }

  return '等待首个成功请求'
}

async function submit() {
  if (!form.name.trim()) {
    error.value = '请输入账号名称'
    return
  }

  if (!form.access_token.trim() || !form.refresh_token.trim()) {
    error.value = '请填写 Access Token 和 Refresh Token'
    return
  }

  if (form.weight < 1) {
    error.value = '权重必须大于等于 1'
    return
  }

  saving.value = true
  error.value = ''
  message.value = ''

  try {
    const account = await apiRequest<AccountOverview>('/api/admin/accounts', {
      method: 'POST',
      body: JSON.stringify({
        ...form,
        token_expires_at: form.token_expires_at || null,
        fingerprint: form.fingerprint || null,
        user_agent: form.user_agent || null,
        metadata: {},
      }),
    })
    accounts.value = [account, ...accounts.value]
    resetForm()
    message.value = `账号 ${account.account.name} 已创建。`
  } catch (err) {
    error.value = err instanceof Error ? err.message : '创建账号失败'
  } finally {
    saving.value = false
  }
}

function handleCodexLoginMessage(event: MessageEvent<CodexLoginMessage>) {
  if (!expectedOauthOrigin.value || event.origin !== expectedOauthOrigin.value) {
    return
  }

  const payload = event.data
  if (!payload || payload.source !== 'subrouter-codex-oauth') {
    return
  }

  expectedOauthOrigin.value = ''
  if (payload.status === 'success') {
    error.value = ''
    message.value = payload.message ?? 'Codex 账号已登录并保存。'
    void loadAccounts()
    return
  }

  error.value = payload.message ?? 'Codex 登录失败'
}

onMounted(() => {
  void loadAccounts()
  window.addEventListener('message', handleCodexLoginMessage)
})

onBeforeUnmount(() => {
  window.removeEventListener('message', handleCodexLoginMessage)
})
</script>

<template>
  <section class="page-section stack-lg">
    <header class="page-header">
      <div>
        <p class="eyebrow">Accounts</p>
        <h2>订阅账号</h2>
      </div>
      <div class="button-row">
        <button
          class="ghost-button"
          type="button"
          :disabled="loading || saving || probingAll || importing || oauthStarting"
          @click="probeAllQuotas"
        >
          {{ probingAll ? 'Probe 中...' : '一键刷新配额' }}
        </button>
        <button
          class="ghost-button"
          type="button"
          :disabled="loading || saving || probingAll || importing || oauthStarting"
          @click="loadAccounts"
        >
          刷新
        </button>
      </div>
    </header>

    <div class="status-stack" aria-live="polite" aria-atomic="true">
      <p v-if="error" class="error-text" role="alert">{{ error }}</p>
      <p v-else-if="message" class="muted" role="status">{{ message }}</p>
    </div>

    <div class="accounts-layout">
      <article class="panel-card stack accounts-main-panel">
        <div class="panel-header">
          <div>
            <h3>当前账号池</h3>
            <p class="muted account-section-copy">把账号池作为主视区，优先观察剩余额度、重置节奏和当前会话占用。</p>
          </div>
          <span class="pill muted-pill">{{ accounts.length }} 个账号</span>
        </div>

        <p v-if="loading" class="muted">正在加载账号列表...</p>
        <p v-else-if="!hasAccounts" class="muted">
          还没有账号。先从右侧粘贴一份 ChatGPT JSON，或者手动录入一个 OAuth 订阅账号。
        </p>

        <div v-else class="account-pool-list">
          <article v-for="item in sortedAccounts" :key="item.account.id" class="account-pool-row">
            <div class="account-row-top">
              <div
                class="quota-bar compact account-top-progress"
                :title="resetProgressTooltip(quotaFor(item.quotas, '7d'))"
                :aria-label="resetProgressTooltip(quotaFor(item.quotas, '7d'))"
              >
                <div
                  class="quota-bar-fill"
                  :data-tone="quotaTone(quotaFor(item.quotas, '7d'))"
                  :style="{ width: resetProgressWidth(quotaFor(item.quotas, '7d')) }"
                />
              </div>
            </div>

            <div class="account-row-name stack-xs">
              <div class="account-row-title-line">
                <h3 class="account-card-title wrap-text">{{ item.account.name }}</h3>
              </div>
              <p class="account-card-subtitle">
                当前会话 {{ item.current_session_count }} · {{ latestAccountNote(item) }}
              </p>
              <p class="account-card-stats">
                成功率 {{ formatSuccessRate(item.usage.success_rate) }} · 输入
                {{ formatNumber(item.usage.total_input_tokens) }} · 输出
                {{ formatNumber(item.usage.total_output_tokens) }}
              </p>
              <p v-if="item.account.last_error" class="account-row-inline-error">
                {{ item.account.last_error }}
              </p>
            </div>

            <div class="account-row-metric stack-xs">
              <div class="account-row-metric-head">
                <span class="quota-label">7 天剩余</span>
                <strong>{{ quotaLabel(quotaFor(item.quotas, '7d'), '待探测') }}</strong>
              </div>
              <div class="quota-bar compact" aria-hidden="true">
                <div
                  class="quota-bar-fill"
                  :data-tone="quotaTone(quotaFor(item.quotas, '7d'))"
                  :style="{ width: quotaFillWidth(quotaFor(item.quotas, '7d')) }"
                />
              </div>
            </div>

            <div class="account-row-metric stack-xs">
              <div class="account-row-metric-head">
                <span class="quota-label">5 小时剩余</span>
                <strong>{{ quotaLabel(quotaFor(item.quotas, '5h'), '待探测') }}</strong>
              </div>
              <div class="quota-bar compact" aria-hidden="true">
                <div
                  class="quota-bar-fill"
                  :data-tone="quotaTone(quotaFor(item.quotas, '5h'))"
                  :style="{ width: quotaFillWidth(quotaFor(item.quotas, '5h')) }"
                />
              </div>
            </div>

            <div class="account-row-action stack-xs">
              <RouterLink class="table-link account-card-link" :to="`/accounts/${item.account.id}`">
                查看详情
              </RouterLink>
            </div>
          </article>
        </div>
      </article>

      <article class="panel-card stack accounts-side-panel">
        <div class="form-card-header">
          <div class="form-card-copy">
            <h3>新增账号</h3>
            <p>右侧保持精简，常用路径是直接从剪贴板导入 ChatGPT JSON。</p>
          </div>
          <button
            class="ghost-button"
            type="button"
            :disabled="saving || probingAll || importing || oauthStarting"
            @click="startCodexLogin"
          >
            {{ oauthStarting ? '准备中...' : '网页登录' }}
          </button>
          <button
            class="ghost-button"
            type="button"
            :disabled="saving || probingAll || importing || oauthStarting"
            @click="pasteFromClipboard"
          >
            {{ importing ? '读取中...' : '粘贴导入' }}
          </button>
        </div>

        <form class="stack" @submit.prevent="submit">
          <div class="form-grid compact-form-grid">
            <label class="field">
              <span>名称</span>
              <input v-model="form.name" type="text" placeholder="例如 main-oauth-01" />
            </label>

            <label class="field">
              <span>权重</span>
              <input v-model.number="form.weight" type="number" min="1" />
            </label>

            <label class="field">
              <span>Token 过期时间</span>
              <input v-model="form.token_expires_at" type="datetime-local" />
            </label>
          </div>

          <p class="paste-note">
            网页登录会沿用这里的名称和权重；如果名称留空，系统会自动生成。
          </p>

          <p class="paste-note">
            支持直接粘贴包含 <code>access_token</code>、<code>refresh_token</code> 的 ChatGPT JSON，自动填充表单。
          </p>

          <label class="field">
            <span>Access Token</span>
            <textarea v-model="form.access_token" rows="4" placeholder="加密存储" />
          </label>

          <label class="field">
            <span>Refresh Token</span>
            <textarea v-model="form.refresh_token" rows="4" placeholder="加密存储" />
          </label>

          <details class="panel-subsection">
            <summary>兼容字段</summary>

            <div class="form-grid compact-form-grid advanced-grid">
              <label class="field">
                <span>Fingerprint</span>
                <input v-model="form.fingerprint" type="text" placeholder="可留空" />
                <small class="field-hint">当前运行时未使用，仅预留给兼容场景。</small>
              </label>

              <label class="field">
                <span>User-Agent</span>
                <input v-model="form.user_agent" type="text" placeholder="可留空" />
                <small class="field-hint">仅在客户端未传 User-Agent 时作为回退值。</small>
              </label>
            </div>
          </details>

          <button
            class="primary-button"
            type="submit"
            :disabled="saving || importing || oauthStarting"
          >
            {{ saving ? '保存中...' : '创建账号' }}
          </button>
        </form>
      </article>
    </div>
  </section>
</template>
