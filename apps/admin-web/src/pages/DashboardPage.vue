<script setup lang="ts">
import { onMounted, ref } from 'vue'

import { apiRequest } from '../lib/api'
import type { DashboardSummary } from '../types'

const loading = ref(true)
const error = ref('')
const summary = ref<DashboardSummary | null>(null)

async function load() {
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

onMounted(load)
</script>

<template>
  <section class="page-section stack-lg">
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
      <div class="card-grid metrics-grid">
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
          <strong>{{ summary.input_tokens_last_24h }}</strong>
          <small>输出 {{ summary.output_tokens_last_24h }}</small>
        </article>
      </div>

      <div class="card-grid two-column-grid">
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
            <h3>阶段说明</h3>
            <span class="pill muted-pill">Phase 4</span>
          </div>
          <p class="muted">
            当前仪表盘已经接通账号、会话、usage 和 quota 峰值聚合。账号详情页支持手动 probe 配额，代理也会自动把上游
            `x-codex-*` 头归一化为 5h 与 7d 快照。
          </p>
        </article>
      </div>
    </template>
  </section>
</template>
