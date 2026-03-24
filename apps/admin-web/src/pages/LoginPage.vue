<script setup lang="ts">
import { ref } from 'vue'

import { useAuthStore } from '../stores/auth'

const auth = useAuthStore()
const token = ref('')

function handleInput() {
  if (auth.error) {
    auth.error = ''
  }
}

async function submit() {
  if (!token.value.trim()) {
    auth.error = '请输入管理员 Token'
    return
  }

  try {
    await auth.login(token.value)
  } catch {
    token.value = ''
  }
}
</script>

<template>
  <section class="login-page">
    <div class="login-card">
      <p class="eyebrow">SubRouter</p>
      <h1>管理员登录</h1>
      <p class="muted">
        使用 `SUBROUTER_ADMIN_TOKEN` 换取本地 session cookie，后续管理操作无需反复贴 token。
      </p>

      <form class="stack" @submit.prevent="submit">
        <label class="field">
          <span>管理员 Token</span>
          <input
            v-model="token"
            required
            type="password"
            placeholder="请输入部署时配置的 Token"
            autocomplete="current-password"
            :aria-invalid="auth.error ? 'true' : 'false'"
            :aria-describedby="auth.error ? 'login-error' : 'login-hint'"
            @input="handleInput"
          />
        </label>
        <p id="login-hint" class="field-hint">
          只在当前浏览器换取本地 session，不会把管理员 Token 常驻显示在界面里。
        </p>

        <p v-if="auth.error" id="login-error" class="error-text" role="alert">
          {{ auth.error }}
        </p>
        <p v-else-if="auth.loading" class="muted" role="status" aria-live="polite">
          正在验证管理员身份...
        </p>

        <button class="primary-button" type="submit" :disabled="auth.loading">
          {{ auth.loading ? '登录中...' : '进入控制台' }}
        </button>
      </form>
    </div>
  </section>
</template>
