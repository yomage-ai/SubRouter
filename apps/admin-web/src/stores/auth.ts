import { defineStore } from 'pinia'
import { router } from '../router'
import { apiRequest, ApiError } from '../lib/api'

interface SessionStatusResponse {
  authenticated: boolean
}

export const useAuthStore = defineStore('auth', {
  state: () => ({
    ready: false,
    authenticated: false,
    loading: false,
    error: '' as string,
  }),
  actions: {
    async bootstrap() {
      if (this.ready) {
        return
      }

      try {
        const session = await apiRequest<SessionStatusResponse>(
          '/api/admin/session/me',
          { method: 'GET' },
        )
        this.authenticated = session.authenticated
      } catch {
        this.authenticated = false
      } finally {
        this.ready = true
      }
    },
    async login(token: string) {
      this.loading = true
      this.error = ''

      try {
        await apiRequest<void>('/api/admin/session/login', {
          method: 'POST',
          body: JSON.stringify({ token }),
        })
        this.authenticated = true
        await router.push({ name: 'dashboard' })
      } catch (error) {
        this.authenticated = false
        this.error =
          error instanceof ApiError ? error.message : '登录失败，请检查后端服务'
        throw error
      } finally {
        this.loading = false
      }
    },
    async logout() {
      try {
        await apiRequest<void>('/api/admin/session/logout', { method: 'POST' })
      } finally {
        this.authenticated = false
        this.error = ''
        await router.push({ name: 'login' })
      }
    },
    async handleUnauthorized() {
      this.authenticated = false
      this.loading = false
      this.error = '登录状态已过期，请重新登录。'

      if (router.currentRoute.value.name !== 'login') {
        await router.push({ name: 'login' })
      }
    },
  },
})
