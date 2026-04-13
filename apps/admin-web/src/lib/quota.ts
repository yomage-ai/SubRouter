import type { QuotaSnapshot } from '../types'

const URGENT_SEVEN_DAY_RESET_MS = 3 * 60 * 60 * 1000

export function quotaFor(
  quotas: QuotaSnapshot[],
  windowType: '5h' | '7d',
): QuotaSnapshot | undefined {
  return quotas.find((quota) => quota.window_type === windowType)
}

export function usedPercent(quota: QuotaSnapshot | undefined): number | null {
  if (!quota) {
    return null
  }

  return clampPercent(quota.used_percent)
}

export function remainingPercent(quota: QuotaSnapshot | undefined): number | null {
  if (!quota) {
    return null
  }

  return clampPercent(100 - quota.used_percent)
}

export function quotaFillWidth(quota: QuotaSnapshot | undefined): string {
  const remaining = remainingPercent(quota)
  return remaining === null ? '0%' : `${remaining}%`
}

export function quotaTone(quota: QuotaSnapshot | undefined): 'healthy' | 'warn' | 'danger' | 'unknown' {
  const remaining = remainingPercent(quota)
  if (remaining === null) {
    return 'unknown'
  }
  if (remaining <= 10) {
    return 'danger'
  }
  if (remaining <= 40) {
    return 'warn'
  }
  return 'healthy'
}

export function remainingPercentText(quota: QuotaSnapshot | undefined): string {
  const remaining = remainingPercent(quota)
  return remaining === null ? '待探测' : `${remaining.toFixed(1)}% 剩余`
}

export function quotaMetaText(
  quota: QuotaSnapshot | undefined,
  formatTimestamp: (value: string | null) => string,
): string {
  if (!quota) {
    return '等待自动观测或手动 Probe'
  }

  const parts = [
    `已用 ${usedPercent(quota)?.toFixed(1)}%`,
    `来源 ${quota.source}`,
    `更新于 ${formatTimestamp(quota.updated_at)}`,
  ]

  if (quota.reset_at) {
    parts.push(`重置 ${formatTimestamp(quota.reset_at)}`)
  }

  return parts.join(' · ')
}

export function resetWindowRemainingPercent(quota: QuotaSnapshot | undefined): number | null {
  if (!quota?.reset_at) {
    return null
  }

  const resetAtMs = new Date(quota.reset_at).getTime()
  if (Number.isNaN(resetAtMs)) {
    return null
  }

  const durationMs = quota.window_type === '5h' ? 5 * 60 * 60 * 1000 : 7 * 24 * 60 * 60 * 1000
  const remainingMs = resetAtMs - Date.now()

  return clampPercent((remainingMs / durationMs) * 100)
}

export function quotaPriorityScore(
  sevenDayQuota: QuotaSnapshot | undefined,
  fiveHourQuota: QuotaSnapshot | undefined,
): number | null {
  const sevenDayRemaining = remainingPercent(sevenDayQuota)
  const resetRemaining = resetWindowRemainingPercent(sevenDayQuota)
  const fiveHourRemaining = remainingPercent(fiveHourQuota)
  if (sevenDayRemaining === null || resetRemaining === null || fiveHourRemaining === null) {
    return null
  }

  const recyclablePressure = sevenDayRemaining * (1 - resetRemaining / 100)
  const fiveHourFactor = 0.5 + fiveHourRemaining / 200

  return recyclablePressure * fiveHourFactor
}

export function isUrgentSevenDayReset(quota: QuotaSnapshot | undefined): boolean {
  if (!quota?.reset_at || quota.window_type !== '7d') {
    return false
  }

  const resetAtMs = new Date(quota.reset_at).getTime()
  if (Number.isNaN(resetAtMs)) {
    return false
  }

  const remainingMs = resetAtMs - Date.now()
  return remainingMs >= 0 && remainingMs <= URGENT_SEVEN_DAY_RESET_MS
}

function clampPercent(value: number): number {
  return Math.min(100, Math.max(0, value))
}
