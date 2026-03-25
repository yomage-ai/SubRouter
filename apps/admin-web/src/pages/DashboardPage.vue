<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'

import { apiRequest } from '../lib/api'
import type {
  DashboardSummary,
  DashboardTokenRange,
  DashboardTokenUsagePoint,
  DashboardTokenUsageSeries,
} from '../types'

const RANGE_OPTIONS = [
  { value: '24h', label: '最近 24 小时', hint: '按小时聚合' },
  { value: '7d', label: '最近 7 天', hint: '按 6 小时聚合' },
  { value: '30d', label: '最近 30 天', hint: '按天聚合' },
] as const satisfies ReadonlyArray<{
  value: DashboardTokenRange
  label: string
  hint: string
}>

const loading = ref(true)
const error = ref('')
const summary = ref<DashboardSummary | null>(null)

const selectedRange = ref<DashboardTokenRange>('24h')
const trendLoading = ref(true)
const trendError = ref('')
const tokenUsage = ref<DashboardTokenUsageSeries | null>(null)

let trendRequestSequence = 0

async function loadSummary() {
  loading.value = true
  error.value = ''

  try {
    summary.value = await apiRequest<DashboardSummary>('/api/admin/dashboard', {
      method: 'GET',
    })
  } catch (err) {
    error.value = err instanceof Error ? err.message : '仪表盘加载失败'
  } finally {
    loading.value = false
  }
}

async function loadTokenUsage(range = selectedRange.value) {
  const requestSequence = ++trendRequestSequence
  trendLoading.value = true
  trendError.value = ''

  try {
    const nextUsage = await apiRequest<DashboardTokenUsageSeries>(
      `/api/admin/dashboard/token-usage?range=${encodeURIComponent(range)}`,
      {
        method: 'GET',
      },
    )

    if (requestSequence !== trendRequestSequence) {
      return
    }

    tokenUsage.value = nextUsage
    selectedRange.value = nextUsage.range
  } catch (err) {
    if (requestSequence !== trendRequestSequence) {
      return
    }

    trendError.value = err instanceof Error ? err.message : 'Token 曲线加载失败'
  } finally {
    if (requestSequence === trendRequestSequence) {
      trendLoading.value = false
    }
  }
}

async function load() {
  await Promise.all([loadSummary(), loadTokenUsage(selectedRange.value)])
}

async function selectRange(range: DashboardTokenRange) {
  if (selectedRange.value === range && tokenUsage.value) {
    return
  }

  selectedRange.value = range
  await loadTokenUsage(range)
}

function formatPercent(value: number | null) {
  if (value === null) {
    return '未知'
  }
  return `${value.toFixed(1)}%`
}

function formatCooldownSummary(count: number) {
  if (count === 0) {
    return '当前没有账号处于 cooldown。'
  }
  if (count === 1) {
    return '当前有 1 个账号因为 quota 命中 cooldown。'
  }
  return `当前有 ${count} 个账号因为 quota 命中 cooldown。`
}

function formatNumber(value: number) {
  return new Intl.NumberFormat('zh-CN').format(value)
}

function formatCompactNumber(value: number) {
  return new Intl.NumberFormat('zh-CN', {
    notation: 'compact',
    maximumFractionDigits: value >= 100000 ? 0 : 1,
  }).format(value)
}

function formatRangeLabel(range: DashboardTokenRange) {
  return RANGE_OPTIONS.find((option) => option.value === range)?.label ?? range
}

function formatBucketLabel(value: string, range: DashboardTokenRange) {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) {
    return '--'
  }

  if (range === '24h') {
    return new Intl.DateTimeFormat('zh-CN', {
      hour: '2-digit',
      minute: '2-digit',
    }).format(date)
  }

  if (range === '7d') {
    return new Intl.DateTimeFormat('zh-CN', {
      month: 'numeric',
      day: 'numeric',
      hour: '2-digit',
    }).format(date)
  }

  return new Intl.DateTimeFormat('zh-CN', {
    month: 'numeric',
    day: 'numeric',
  }).format(date)
}

function formatBucketInterval(range: DashboardTokenRange) {
  if (range === '24h') {
    return '1 小时'
  }
  if (range === '7d') {
    return '6 小时'
  }
  return '1 天'
}

const currentRangeMeta = computed(
  () => RANGE_OPTIONS.find((option) => option.value === selectedRange.value) ?? RANGE_OPTIONS[0],
)

const trendPoints = computed(() => tokenUsage.value?.points ?? [])

const hasTrendData = computed(() => trendPoints.value.some((point) => point.total_tokens > 0))

const totalTokensInRange = computed(
  () => (tokenUsage.value?.total_input_tokens ?? 0) + (tokenUsage.value?.total_output_tokens ?? 0),
)

const peakPoint = computed<DashboardTokenUsagePoint | null>(() => {
  let currentPeak: DashboardTokenUsagePoint | null = null

  for (const point of trendPoints.value) {
    if (!currentPeak || point.total_tokens > currentPeak.total_tokens) {
      currentPeak = point
    }
  }

  return currentPeak
})

const latestPoint = computed<DashboardTokenUsagePoint | null>(
  () => trendPoints.value.at(-1) ?? null,
)

const averageTokensPerBucket = computed(() => {
  if (trendPoints.value.length === 0) {
    return 0
  }

  return Math.round(totalTokensInRange.value / trendPoints.value.length)
})

const chartMax = computed(() =>
  Math.max(
    1,
    ...trendPoints.value.map((point) => point.total_tokens),
  ),
)

function chartX(index: number, total: number) {
  if (total <= 1) {
    return 50
  }

  return 2 + (index * 96) / (total - 1)
}

function chartY(totalTokens: number, maxValue: number) {
  const top = 6
  const bottom = 44
  return bottom - (totalTokens / maxValue) * (bottom - top)
}

const plottedPoints = computed(() => {
  const points = trendPoints.value
  const maxValue = chartMax.value

  return points.map((point, index) => ({
    ...point,
    key: `${point.bucket_start}-${index}`,
    x: chartX(index, points.length),
    y: chartY(point.total_tokens, maxValue),
  }))
})

const chartGuides = computed(() => {
  const maxValue = chartMax.value
  return [
    { key: 'top', rawValue: maxValue },
    { key: 'mid', rawValue: maxValue / 2 },
    { key: 'bottom', rawValue: 0 },
  ].map((guide) => ({
    key: guide.key,
    value: Math.round(guide.rawValue),
    y: chartY(guide.rawValue, maxValue),
  }))
})

function buildChartCurvePath(
  points: Array<{ x: number; y: number }>,
  baselineY?: number,
) {
  if (points.length === 0) {
    return ''
  }

  if (points.length === 1) {
    const point = points[0]
    const linePath = `M ${point.x.toFixed(2)} ${point.y.toFixed(2)}`
    if (baselineY === undefined) {
      return linePath
    }

    return `${linePath} L ${point.x.toFixed(2)} ${baselineY.toFixed(2)} Z`
  }

  let path = `M ${points[0].x.toFixed(2)} ${points[0].y.toFixed(2)}`

  for (let index = 0; index < points.length - 1; index += 1) {
    const current = points[index]
    const next = points[index + 1]
    const midpointX = (current.x + next.x) / 2

    path += ` C ${midpointX.toFixed(2)} ${current.y.toFixed(2)}, ${midpointX.toFixed(2)} ${next.y.toFixed(2)}, ${next.x.toFixed(2)} ${next.y.toFixed(2)}`
  }

  if (baselineY === undefined) {
    return path
  }

  const last = points[points.length - 1]
  const first = points[0]
  return `${path} L ${last.x.toFixed(2)} ${baselineY.toFixed(2)} L ${first.x.toFixed(2)} ${baselineY.toFixed(2)} Z`
}

const chartLinePath = computed(() => {
  if (plottedPoints.value.length === 0) {
    return ''
  }

  return buildChartCurvePath(plottedPoints.value)
})

const chartAreaPath = computed(() => {
  if (plottedPoints.value.length === 0) {
    return ''
  }

  return buildChartCurvePath(plottedPoints.value, 44)
})

const axisLabels = computed(() => {
  const points = trendPoints.value
  if (points.length === 0) {
    return []
  }

  const indexes = Array.from(new Set([0, Math.floor((points.length - 1) / 2), points.length - 1]))
  return indexes.map((index) => ({
    key: `${points[index].bucket_start}-${index}`,
    label: formatBucketLabel(points[index].bucket_start, selectedRange.value),
  }))
})

const chartDescription = computed(() => {
  if (!tokenUsage.value || trendPoints.value.length === 0) {
    return '当前没有可展示的 token 消耗数据。'
  }

  const peak = peakPoint.value
  const peakText = peak
    ? `峰值出现在 ${formatBucketLabel(peak.bucket_start, selectedRange.value)}，总计 ${formatNumber(peak.total_tokens)} token。`
    : '当前没有峰值数据。'

  return `${formatRangeLabel(selectedRange.value)} 共计 ${formatNumber(totalTokensInRange.value)} token，${peakText}`
})

const highlightedPointKeys = computed(() => {
  const keys = new Set<string>()

  if (peakPoint.value) {
    const peak = plottedPoints.value.find((point) => point.bucket_start === peakPoint.value?.bucket_start)
    if (peak) {
      keys.add(peak.key)
    }
  }

  const latest = plottedPoints.value.at(-1)
  if (latest) {
    keys.add(latest.key)
  }

  return keys
})

function isHighlightedPoint(key: string) {
  return highlightedPointKeys.value.has(key)
}

onMounted(load)
</script>

<template>
  <section class="page-section stack-lg dashboard-page">
    <header class="page-header">
      <div>
        <p class="eyebrow">Overview</p>
        <h2>配额与负载仪表盘</h2>
      </div>
      <button class="ghost-button" type="button" @click="load">刷新</button>
    </header>

    <div v-if="error || loading || !summary" class="status-stack" aria-live="polite" aria-atomic="true">
      <p v-if="error" class="error-text" role="alert">{{ error }}</p>
      <p v-else-if="loading" class="muted" role="status">正在加载仪表盘...</p>
      <p v-else class="muted" role="status">暂无仪表盘数据。</p>
    </div>

    <template v-if="summary && !error && !loading">
      <div class="card-grid metrics-grid dashboard-metrics-grid">
        <article class="metric-card accent-card">
          <span class="metric-label">订阅账号</span>
          <strong>{{ summary.total_accounts }}</strong>
          <small>活跃 {{ summary.active_accounts }} / 停用 {{ summary.disabled_accounts }}</small>
        </article>

        <article class="metric-card teal-card">
          <span class="metric-label">活跃会话</span>
          <strong>{{ summary.total_active_sessions }}</strong>
          <small>冷却账号 {{ summary.accounts_in_cooldown }}</small>
        </article>

        <article class="metric-card">
          <span class="metric-label">近 24h 请求</span>
          <strong>{{ summary.requests_last_24h }}</strong>
          <small>refresh_failed {{ summary.refresh_failed_accounts }}</small>
        </article>

        <article class="metric-card">
          <span class="metric-label">近 24h 输入 Token</span>
          <strong>{{ formatNumber(summary.input_tokens_last_24h) }}</strong>
          <small>输出 {{ formatNumber(summary.output_tokens_last_24h) }}</small>
        </article>
      </div>

      <div class="dashboard-layout">
        <article class="panel-card chart-panel stack">
          <div class="chart-panel-header">
            <div class="stack-sm chart-copy-block">
              <div class="panel-header">
                <h3>Token 消耗曲线</h3>
                <span class="pill">Usage</span>
              </div>
              <p class="muted chart-copy">
                基于 <code>usage_events</code> 聚合时间序列，直接看最近窗口里的吞吐波动和峰值位置。
              </p>
            </div>

            <div class="segment-control" role="tablist" aria-label="Token 消耗时间范围">
              <button
                v-for="option in RANGE_OPTIONS"
                :key="option.value"
                class="segment-button"
                type="button"
                role="tab"
                :data-active="selectedRange === option.value"
                :aria-selected="selectedRange === option.value"
                :disabled="trendLoading && selectedRange === option.value"
                @click="selectRange(option.value)"
              >
                <span>{{ option.label }}</span>
                <small>{{ option.hint }}</small>
              </button>
            </div>
          </div>

          <div class="chart-stat-strip">
            <article class="chart-kpi">
              <span class="chart-kpi-label">当前窗口总量</span>
              <strong>{{ formatCompactNumber(totalTokensInRange) }}</strong>
              <small>输入 {{ formatCompactNumber(tokenUsage?.total_input_tokens ?? 0) }}</small>
            </article>

            <article class="chart-kpi">
              <span class="chart-kpi-label">输出 Token</span>
              <strong>{{ formatCompactNumber(tokenUsage?.total_output_tokens ?? 0) }}</strong>
              <small>{{ currentRangeMeta.label }}</small>
            </article>

            <article class="chart-kpi">
              <span class="chart-kpi-label">窗口峰值</span>
              <strong>{{ formatCompactNumber(peakPoint?.total_tokens ?? 0) }}</strong>
              <small>
                {{ peakPoint ? formatBucketLabel(peakPoint.bucket_start, selectedRange) : '等待数据' }}
              </small>
            </article>

            <article class="chart-kpi">
              <span class="chart-kpi-label">平均每桶</span>
              <strong>{{ formatCompactNumber(averageTokensPerBucket) }}</strong>
              <small>{{ formatBucketInterval(selectedRange) }} / 桶</small>
            </article>
          </div>

          <div
            v-if="trendError || trendLoading || !tokenUsage"
            class="chart-shell chart-empty"
            aria-live="polite"
            aria-atomic="true"
          >
            <p v-if="trendError" class="error-text" role="alert">{{ trendError }}</p>
            <p v-else-if="trendLoading" class="muted" role="status">正在加载 token 曲线...</p>
            <p v-else class="muted" role="status">暂无 token 曲线数据。</p>
          </div>

          <div v-else-if="!hasTrendData" class="chart-shell chart-empty">
            <div class="stack-sm">
              <strong>这个时间窗口里还没有 token 消耗</strong>
              <p class="muted">
                先发起几次请求，图表会按 {{ currentRangeMeta.hint }} 自动聚合并补出走势。
              </p>
            </div>
          </div>

          <div v-else class="chart-shell stack-sm">
            <div class="chart-y-labels" aria-hidden="true">
              <span v-for="guide in chartGuides" :key="guide.key">{{ formatCompactNumber(guide.value) }}</span>
            </div>

            <svg
              class="token-chart"
              viewBox="0 0 100 50"
              role="img"
              aria-labelledby="token-usage-chart-title token-usage-chart-desc"
            >
              <title id="token-usage-chart-title">Token 消耗曲线</title>
              <desc id="token-usage-chart-desc">{{ chartDescription }}</desc>

              <g class="chart-grid" aria-hidden="true">
                <line
                  v-for="guide in chartGuides"
                  :key="guide.key"
                  x1="2"
                  :y1="guide.y"
                  x2="98"
                  :y2="guide.y"
                />
              </g>

              <path class="chart-area" :d="chartAreaPath" />
              <path class="chart-line" :d="chartLinePath" />

              <circle
                v-for="point in plottedPoints.filter((item) => isHighlightedPoint(item.key))"
                :key="point.key"
                class="chart-dot chart-dot-highlight"
                :cx="point.x"
                :cy="point.y"
                r="0.84"
              />
            </svg>

            <div class="chart-footer" aria-hidden="true">
              <span v-for="label in axisLabels" :key="label.key">{{ label.label }}</span>
            </div>

            <div class="chart-legend">
              <span><i class="legend-swatch total" />总 token {{ formatNumber(totalTokensInRange) }}</span>
              <span><i class="legend-swatch input" />输入 {{ formatNumber(tokenUsage.total_input_tokens) }}</span>
              <span><i class="legend-swatch output" />输出 {{ formatNumber(tokenUsage.total_output_tokens) }}</span>
              <span><i class="legend-swatch requests" />请求 {{ formatNumber(tokenUsage.total_requests) }}</span>
            </div>
          </div>
        </article>

        <div class="dashboard-sidebar">
          <article class="panel-card stack">
            <div class="panel-header">
              <h3>窗口利用率</h3>
              <span class="pill">Quota</span>
            </div>
            <div class="stat-row">
              <span>5 小时峰值</span>
              <strong>{{ formatPercent(summary.highest_five_hour_usage) }}</strong>
            </div>
            <div class="stat-row">
              <span>7 天峰值</span>
              <strong>{{ formatPercent(summary.highest_seven_day_usage) }}</strong>
            </div>
            <p class="muted">{{ formatCooldownSummary(summary.accounts_in_cooldown) }}</p>
          </article>

          <article class="panel-card stack">
            <div class="panel-header">
              <h3>窗口摘要</h3>
              <span class="pill muted-pill">{{ currentRangeMeta.label }}</span>
            </div>
            <div class="stat-row">
              <span>聚合粒度</span>
              <strong>{{ formatBucketInterval(selectedRange) }}</strong>
            </div>
            <div class="stat-row">
              <span>最近一个桶</span>
              <strong>
                {{ latestPoint ? formatCompactNumber(latestPoint.total_tokens) : '0' }}
              </strong>
            </div>
            <div class="stat-row">
              <span>峰值时间</span>
              <strong>
                {{ peakPoint ? formatBucketLabel(peakPoint.bucket_start, selectedRange) : '等待数据' }}
              </strong>
            </div>
            <p class="muted">
              这个面板更适合看趋势而不是绝对精度。需要逐条核对时，仍然以账号详情里的近期事件为准。
            </p>
          </article>
        </div>
      </div>
    </template>
  </section>
</template>
