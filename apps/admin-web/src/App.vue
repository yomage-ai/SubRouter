<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted } from 'vue'
import { RouterLink, RouterView, useRoute } from 'vue-router'

import { useAuthStore } from './stores/auth'
import { UNAUTHORIZED_EVENT } from './lib/api'

const auth = useAuthStore()
const route = useRoute()

const showShell = computed(() => auth.authenticated && route.name !== 'login')

function handleUnauthorized() {
  void auth.handleUnauthorized()
}

onMounted(() => {
  window.addEventListener(UNAUTHORIZED_EVENT, handleUnauthorized)
})

onBeforeUnmount(() => {
  window.removeEventListener(UNAUTHORIZED_EVENT, handleUnauthorized)
})
</script>

<template>
  <div class="app-shell">
    <template v-if="showShell">
      <aside class="app-sidebar">
        <div>
          <p class="eyebrow">SubRouter</p>
          <h1>Admin Console</h1>
          <p class="muted">
            管理 OpenAI OAuth 订阅账号、查看配额和观测 token 使用情况。
          </p>
        </div>

        <nav class="nav-links">
          <RouterLink to="/dashboard">仪表盘</RouterLink>
          <RouterLink to="/accounts">订阅账号</RouterLink>
        </nav>

        <button class="ghost-button" type="button" @click="auth.logout()">
          退出登录
        </button>
      </aside>

      <main class="app-main">
        <RouterView />
      </main>
    </template>

    <template v-else>
      <RouterView />
    </template>
  </div>
</template>
