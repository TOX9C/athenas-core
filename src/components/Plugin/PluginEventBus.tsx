import { useEffect } from 'react'
import { useNotificationStore, isEnhanced } from '../../store/notificationStore'
import { useAgentStatusStore } from '../../store/agentStatusStore'
import { showNotificationToast } from '../Notifications/NotificationToast'
import type { PluginEvent, PluginEventPayload } from '../../types/plugin'
import type { EnhancedNotification } from '../../store/notificationStore'
import type { AgentType } from '../../types/workspace'

function agentTypeFromEvent(event: PluginEvent): AgentType | undefined {
  return event.source?.agentType as AgentType | undefined
}

function nullToUndef<T>(v: T | null | undefined): T | undefined {
  return v ?? undefined
}

function handlePluginEvent(event: PluginEvent) {
  const { type, payload } = event
  const store = useNotificationStore.getState()
  const statusStore = useAgentStatusStore.getState()

  switch (type) {
    case 'notification': {
      const notification: EnhancedNotification = {
        id: event.id ?? Math.random().toString(36).slice(2),
        type: (payload.level as EnhancedNotification['type']) ?? 'info',
        priority: payload.priority ?? 'normal',
        title: payload.title ?? '',
        message: payload.message ?? '',
        timestamp: Date.now(),
        read: false,
        dismissed: false,
        source: event.source?.sessionId ?? 'unknown',
        agentType: agentTypeFromEvent(event),
        spaceId: nullToUndef(event.source?.paneId),
        paneId: nullToUndef(event.source?.paneId),
      }
      store.addEnhancedNotification(notification)

      if (!store.muted && payload.level !== 'info') {
        showNotificationToast({
          type: notification.type,
          title: notification.title,
          message: notification.message,
          agentType: notification.agentType,
        })
      }
      break
    }

    case 'status_update': {
      if (event.source?.paneId) {
        statusStore.updateStatus(event.source.paneId, {
          status: payload.status as any,
          message: payload.message,
          progress: payload.progress,
        })
      }
      const agentEntry = store.agentStatuses.find(
        (a) => a.paneId === event.source?.paneId || a.id === event.source?.agentId,
      )
      if (agentEntry) {
        store.updateAgentStatus(agentEntry.id, {
          status: payload.status as any,
          lastAction: payload.message ?? payload.status,
          lastActionAt: Date.now(),
        })
      }
      break
    }

    case 'needs_input': {
      const notification: EnhancedNotification = {
        id: event.id ?? Math.random().toString(36).slice(2),
        type: 'needs_input',
        priority: 'high',
        title: 'Input Required',
        message: payload.prompt ?? '',
        timestamp: Date.now(),
        read: false,
        dismissed: false,
        source: event.source?.sessionId ?? 'unknown',
        agentType: agentTypeFromEvent(event),
        spaceId: nullToUndef(event.source?.paneId),
        paneId: nullToUndef(event.source?.paneId),
        inputRequestId: payload.requestId,
        inputRequestPrompt: payload.prompt,
        inputRequestOptions: payload.options,
      }
      store.addEnhancedNotification(notification)

      showNotificationToast({
        type: 'needs_input',
        title: 'Input Required',
        message: payload.prompt ?? '',
        agentType: notification.agentType,
        duration: 0,
      })
      break
    }

    case 'task_complete': {
      const notification: EnhancedNotification = {
        id: event.id ?? Math.random().toString(36).slice(2),
        type: 'task_complete',
        priority: 'normal',
        title: 'Task Complete',
        message: payload.message ?? payload.result ?? 'Task finished',
        timestamp: Date.now(),
        read: false,
        dismissed: false,
        source: event.source?.sessionId ?? 'unknown',
        agentType: agentTypeFromEvent(event),
        spaceId: nullToUndef(event.source?.paneId),
        paneId: nullToUndef(event.source?.paneId),
      }
      store.addEnhancedNotification(notification)

      showNotificationToast({
        type: 'task_complete',
        title: 'Task Complete',
        message: notification.message,
        agentType: notification.agentType,
      })
      break
    }

    case 'agent_connected': {
      const entry = {
        id: payload.agentId ?? event.source?.agentId ?? Math.random().toString(36).slice(2),
        name: payload.name ?? 'Agent',
        agentType: (payload.agentType ?? event.source?.agentType ?? 'custom') as AgentType,
        status: 'idle' as const,
        lastAction: 'Connected',
        lastActionAt: Date.now(),
        connectedAt: Date.now(),
      }
      const addFn = store.addAgentStatusEntry
      if (addFn) {
        addFn(entry)
      }
      break
    }

    case 'agent_disconnected': {
      const agentId = payload.agentId ?? event.source?.agentId
      if (agentId) {
        store.removeAgentStatus(agentId)
      }
      if (event.source?.paneId) {
        statusStore.updateStatus(event.source.paneId, { status: 'disconnected' as any })
      }
      break
    }

    case 'plugin_registered': {
      store.addPlugin({
        id: payload.pluginId ?? '',
        name: payload.name ?? '',
        description: payload.description ?? '',
        version: payload.version ?? '0.0.0',
        author: payload.author ?? '',
        status: 'active',
        enabled: true,
        installedAt: Date.now(),
        updatedAt: Date.now(),
        agentCount: 0,
        capabilities: payload.capabilities ?? [],
      })
      break
    }

    case 'plugin_error': {
      store.setPluginStatus(payload.pluginId ?? '', 'error')
      store.updatePlugin(payload.pluginId ?? '', { error: payload.error })
      break
    }

    default:
      console.debug('[PluginEventBus] Unhandled event type:', type)
  }
}

export function PluginEventBus() {
  useEffect(() => {
    if (!window.athena?.plugin?.onEvent) {
      console.debug('[PluginEventBus] No plugin IPC bridge available')
      return
    }

    const unsub = window.athena.plugin.onEvent((event: PluginEvent) => {
      handlePluginEvent(event)
    })

    let cancelled = false
    const loadPlugins = (retries = 3) => {
      window.athena.plugin
        .list()
        .then((registry: Record<string, any>) => {
          if (cancelled) return
          if (registry && typeof registry === 'object') {
            const plugins = Object.entries(registry).map(([id, entry]) => ({
              id,
              name: entry.name ?? id,
              description: entry.description ?? '',
              version: entry.version ?? '0.0.0',
              author: entry.author ?? '',
              status:
                entry.status === 'enabled'
                  ? ('active' as const)
                  : entry.status === 'error'
                    ? ('error' as const)
                    : ('inactive' as const),
              enabled: entry.status === 'enabled',
              installedAt: Date.now(),
              updatedAt: Date.now(),
              agentCount: 0,
              capabilities: entry.capabilities ?? [],
              error: entry.error,
              config: entry.config,
            }))
            useNotificationStore.getState().setPlugins(plugins)
          }
        })
        .catch(() => {
          if (cancelled || retries <= 0) return
          setTimeout(() => loadPlugins(retries - 1), 500)
        })
    }
    loadPlugins()

    return () => {
      cancelled = true
      unsub?.()
    }
  }, [])

  return null
}
