export class ApiError extends Error {
  status: number

  constructor(status: number, message: string) {
    super(message)
    this.name = 'ApiError'
    this.status = status
  }
}

export const UNAUTHORIZED_EVENT = 'subrouter:unauthorized'

export async function apiRequest<T>(
  path: string,
  init: RequestInit = {},
): Promise<T> {
  let response: Response

  try {
    response = await fetch(path, {
      credentials: 'include',
      headers: {
        'Content-Type': 'application/json',
        ...(init.headers ?? {}),
      },
      ...init,
    })
  } catch (error) {
    throw new ApiError(
      0,
      error instanceof Error
        ? `网络请求失败，${error.message}`
        : '网络请求失败，请检查服务是否可用后重试',
    )
  }

  if (response.status === 204) {
    return undefined as T
  }

  const text = await response.text()
  const payload = safeParseJson(text)

  if (!response.ok) {
    if (shouldBroadcastUnauthorized(path, response.status) && typeof window !== 'undefined') {
      window.dispatchEvent(new Event(UNAUTHORIZED_EVENT))
    }

    throw new ApiError(response.status, payload?.error ?? 'Unknown API error')
  }

  return payload as T
}

function safeParseJson(text: string) {
  if (!text) {
    return null
  }

  try {
    return JSON.parse(text) as { error?: string }
  } catch {
    return { error: text }
  }
}

function shouldBroadcastUnauthorized(path: string, status: number) {
  if (status !== 401) {
    return false
  }

  return !path.endsWith('/session/login') && !path.endsWith('/session/me')
}
