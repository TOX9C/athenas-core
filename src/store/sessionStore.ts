import { create } from 'zustand'
import { nanoid } from 'nanoid'
import type { AthenaMessage } from './athenaStore'

interface SessionListItem {
  id: string
  title: string
  createdAt: number
  updatedAt: number
  messageCount: number
  lastMessagePreview: string
}

interface SessionState {
  sessions: SessionListItem[]
  activeSessionId: string | null
  activeSessionMessages: AthenaMessage[]
  isSessionsLoaded: boolean

  loadSessions: () => Promise<void>
  createSession: (title?: string) => Promise<string | null>
  switchSession: (id: string) => Promise<void>
  deleteSession: (id: string) => Promise<void>
  addMessageToActiveSession: (msg: AthenaMessage) => Promise<void>
  updateSessionTitle: (id: string, title: string) => Promise<void>
  newSession: () => Promise<string | null>
}

function persistLastSessionId(id: string | null) {
  window.athena.store.set('athena-lastSessionId', id).catch(() => {})
}

export const useSessionStore = create<SessionState>((set, get) => ({
  sessions: [],
  activeSessionId: null,
  activeSessionMessages: [],
  isSessionsLoaded: false,

  loadSessions: async () => {
    try {
      const sessions = await window.athena.session.list()
      let lastId: string | null = null
      try {
        lastId = (await window.athena.store.get('athena-lastSessionId')) as string | null
      } catch {}
      set({ sessions, isSessionsLoaded: true })
      if (lastId && sessions.some((s) => s.id === lastId)) {
        await get().switchSession(lastId)
      }
    } catch {
      set({ isSessionsLoaded: true })
    }
  },

  createSession: async (title?: string) => {
    try {
      if (!window.athena?.session?.create) {
        console.error('[sessionStore] window.athena.session.create not available')
        return null
      }
      const session = await window.athena.session.create(title)
      const { useAthenaStore } = await import('./athenaStore')
      useAthenaStore.getState().clearMessages()
      set((s) => ({
        sessions: [
          {
            id: session.id,
            title: session.title,
            createdAt: session.createdAt,
            updatedAt: session.updatedAt,
            messageCount: 0,
            lastMessagePreview: '',
          },
          ...s.sessions,
        ],
        activeSessionId: session.id,
        activeSessionMessages: [],
      }))
      persistLastSessionId(session.id)
      return session.id
    } catch {
      return null
    }
  },

  switchSession: async (id: string) => {
    try {
      const { activeSessionId, activeSessionMessages } = get()
      if (activeSessionId && activeSessionId !== id && activeSessionMessages.length > 0) {
        try {
          const { useAthenaStore } = await import('./athenaStore')
          const currentMessages = useAthenaStore.getState().messages
          if (currentMessages.length > 0) {
            await window.athena.session.update(activeSessionId, { messages: currentMessages })
          }
        } catch {}
      }

      const session = await window.athena.session.get(id)
      if (!session) return
      const mapped = session.messages.map((m) => ({
        id: m.id,
        role: m.role,
        content: m.content,
        timestamp: m.timestamp,
        isError: m.isError,
        images: m.images,
      })) as AthenaMessage[]

      const { useAthenaStore } = await import('./athenaStore')
      useAthenaStore.getState().setMessages(mapped)

      set({
        activeSessionId: id,
        activeSessionMessages: mapped,
      })
      persistLastSessionId(id)
    } catch {
      // ignore
    }
  },

  deleteSession: async (id: string) => {
    try {
      await window.athena.session.delete(id)
      const { activeSessionId, sessions } = get()
      const remaining = sessions.filter((s) => s.id !== id)
      if (activeSessionId === id) {
        const { useAthenaStore } = await import('./athenaStore')
        useAthenaStore.getState().clearMessages()
        if (remaining.length > 0) {
          const nextSession = remaining[0]
          set({ sessions: remaining, activeSessionId: nextSession.id, activeSessionMessages: [] })
          persistLastSessionId(nextSession.id)
          await get().switchSession(nextSession.id)
        } else {
          set({ sessions: remaining, activeSessionId: null, activeSessionMessages: [] })
          persistLastSessionId(null)
        }
      } else {
        set({ sessions: remaining })
      }
    } catch {
      // ignore
    }
  },

  addMessageToActiveSession: async (msg: AthenaMessage) => {
    const { activeSessionId } = get()
    if (!activeSessionId) return
    try {
      const ipcImages = msg.images
        ?.filter((img) => img.base64 && img.base64.length > 0)
        .map((img) => ({ base64: img.base64, mediaType: img.mediaType }))

      const updated = await window.athena.session.addMessage(activeSessionId, {
        id: msg.id,
        role: msg.role,
        content: msg.content,
        timestamp: msg.timestamp,
        isError: msg.isError,
        images: ipcImages && ipcImages.length > 0 ? ipcImages : undefined,
      })
      set((s) => ({
        activeSessionMessages: [...s.activeSessionMessages, msg],
        sessions: s.sessions.map((sess) =>
          sess.id === activeSessionId
            ? {
                ...sess,
                updatedAt: Date.now(),
                messageCount: (sess.messageCount || 0) + 1,
                lastMessagePreview: msg.content.slice(0, 100),
                title: updated?.title ?? sess.title,
              }
            : sess,
        ),
      }))
    } catch {
      set((s) => ({
        activeSessionMessages: [...s.activeSessionMessages, msg],
      }))
    }
  },

  updateSessionTitle: async (id: string, title: string) => {
    try {
      await window.athena.session.update(id, { title })
      set((s) => ({
        sessions: s.sessions.map((sess) => (sess.id === id ? { ...sess, title } : sess)),
      }))
    } catch {
      // ignore
    }
  },

  newSession: async () => {
    return get().createSession()
  },
}))
