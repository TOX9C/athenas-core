import { X, Terminal, Activity, Bell, Search } from 'lucide-react'
import { useState, useEffect, useCallback } from 'react'
import { useAgentOutputStore } from '../../store/agentOutputStore'
import { useAgentStatusStore } from '../../store/agentStatusStore'
import { useNotificationStore, isEnhanced } from '../../store/notificationStore'
import { AgentOutputPanel } from './AgentOutputPanel'
import { AgentSelector } from './AgentSelector'

type InspectorTab = 'output' | 'status' | 'notifications'

export function AgentInspector() {
  const inspectorOpen = useAgentOutputStore((s) => s.inspectorOpen)
  const setInspectorOpen = useAgentOutputStore((s) => s.setInspectorOpen)
  const selectedPaneId = useAgentOutputStore((s) => s.selectedPaneId)
  const selectAgent = useAgentOutputStore((s) => s.selectAgent)
  const setSubscription = useAgentOutputStore((s) => s.setSubscription)
  const clearSubscription = useAgentOutputStore((s) => s.clearSubscription)
  const subscription = useAgentOutputStore((s) => s.subscription)
  const setLines = useAgentOutputStore((s) => s.setLines)
  const appendLine = useAgentOutputStore((s) => s.appendLine)
  const setAgents = useAgentOutputStore((s) => s.setAgents)

  const [tab, setTab] = useState<InspectorTab>('output')
  const [searchQuery, setSearchQuery] = useState('')

  const paneStatus = useAgentStatusStore((s) =>
    selectedPaneId ? s.statuses[selectedPaneId] : undefined,
  )
  const notifications = useNotificationStore((s) => s.notifications)

  const filteredNotifications = selectedPaneId
    ? notifications.filter((n) => {
        if (!isEnhanced(n)) return false
        if (n.paneId !== selectedPaneId) return false
        if (
          searchQuery &&
          !n.message.toLowerCase().includes(searchQuery.toLowerCase()) &&
          !n.title.toLowerCase().includes(searchQuery.toLowerCase())
        )
          return false
        return true
      })
    : []

  const refreshAgents = useCallback(async () => {
    try {
      const list = await window.athena.outputCapture.listAgents()
      setAgents(list)
    } catch {}
  }, [setAgents])

  const subscribeToOutput = useCallback(
    async (paneId: string) => {
      if (subscription.active && subscription.paneId === paneId) return
      if (subscription.active && subscription.subscriptionId) {
        window.athena.outputCapture.unsubscribe(subscription.subscriptionId)
      }
      try {
        const result = await window.athena.outputCapture.subscribe(paneId)
        setSubscription({ subscriptionId: result.subscriptionId, paneId, active: true })
        const lines = await window.athena.outputCapture.read(paneId, { limit: 200 })
        setLines(paneId, lines)
      } catch {}
    },
    [subscription, setSubscription, setLines],
  )

  useEffect(() => {
    if (!inspectorOpen) return
    refreshAgents()
    const interval = setInterval(refreshAgents, 5000)
    return () => clearInterval(interval)
  }, [inspectorOpen, refreshAgents])

  useEffect(() => {
    if (inspectorOpen && selectedPaneId) {
      subscribeToOutput(selectedPaneId)
    }
    if (!inspectorOpen && subscription.active && subscription.subscriptionId) {
      window.athena.outputCapture.unsubscribe(subscription.subscriptionId)
      clearSubscription()
    }
  }, [
    inspectorOpen,
    selectedPaneId,
    subscribeToOutput,
    clearSubscription,
    subscription.active,
    subscription.subscriptionId,
  ])

  useEffect(() => {
    const unsub = window.athena.outputCapture.onLine((data) => {
      appendLine(data.line)
    })
    return unsub
  }, [appendLine])

  if (!inspectorOpen) return null

  return (
    <div
      className="flex flex-col border-l shrink-0"
      style={{ width: 360, background: 'var(--bgSecondary)', borderColor: 'var(--border)' }}
    >
      <div
        className="flex items-center gap-1 px-2 py-1.5 border-b shrink-0"
        style={{ borderColor: 'var(--border)' }}
      >
        <AgentSelector />
        <div className="flex-1" />
        <button
          onClick={() => setInspectorOpen(false)}
          className="p-1 rounded hover:bg-white/10 transition-colors"
          title="Close inspector"
        >
          <X size={12} style={{ color: 'var(--textDim)' }} />
        </button>
      </div>

      <div
        className="flex items-center gap-0.5 px-2 py-1 border-b shrink-0"
        style={{ borderColor: 'var(--border)' }}
      >
        {[
          { id: 'output' as InspectorTab, icon: Terminal, label: 'Output' },
          { id: 'status' as InspectorTab, icon: Activity, label: 'Status' },
          { id: 'notifications' as InspectorTab, icon: Bell, label: 'Alerts' },
        ].map(({ id, icon: Icon, label }) => (
          <button
            key={id}
            onClick={() => setTab(id)}
            className="flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-medium transition-colors"
            style={{
              background: tab === id ? 'var(--bgTertiary)' : 'transparent',
              color: tab === id ? 'var(--text)' : 'var(--textDim)',
            }}
          >
            <Icon size={10} />
            {label}
          </button>
        ))}
      </div>

      <div className="flex-1 min-h-0 overflow-hidden">
        {tab === 'output' && <AgentOutputPanel />}

        {tab === 'status' && (
          <div className="p-3 overflow-y-auto h-full">
            {paneStatus ? (
              <div className="space-y-2">
                <StatusRow label="Pane" value={paneStatus.paneId} />
                <StatusRow label="Status" value={paneStatus.status} />
                {paneStatus.message && <StatusRow label="Message" value={paneStatus.message} />}
                {paneStatus.progress && (
                  <div>
                    <div className="text-[9px] mb-1" style={{ color: 'var(--textDim)' }}>
                      Progress
                    </div>
                    <div className="flex items-center gap-2">
                      <div
                        className="flex-1 h-1 rounded-full overflow-hidden"
                        style={{ background: 'var(--bgTertiary)' }}
                      >
                        <div
                          className="h-full rounded-full transition-all"
                          style={{
                            width: `${Math.min(100, (paneStatus.progress.current / paneStatus.progress.total) * 100)}%`,
                            background: 'var(--accent)',
                          }}
                        />
                      </div>
                      <span className="text-[9px]" style={{ color: 'var(--textDim)' }}>
                        {paneStatus.progress.current}/{paneStatus.progress.total}
                      </span>
                    </div>
                    {paneStatus.progress.label && (
                      <span className="text-[8px] block mt-0.5" style={{ color: 'var(--textDim)' }}>
                        {paneStatus.progress.label}
                      </span>
                    )}
                  </div>
                )}
                <StatusRow
                  label="Last update"
                  value={new Date(paneStatus.lastUpdatedAt).toLocaleTimeString()}
                />
              </div>
            ) : (
              <div
                className="flex items-center justify-center h-full"
                style={{ color: 'var(--textDim)' }}
              >
                <span className="text-[10px]">Select an agent to view status</span>
              </div>
            )}
          </div>
        )}

        {tab === 'notifications' && (
          <div className="flex flex-col h-full">
            <div className="px-2 py-1 border-b shrink-0" style={{ borderColor: 'var(--border)' }}>
              <div
                className="flex items-center gap-1 px-2 py-1 rounded"
                style={{ background: 'var(--bgTertiary)' }}
              >
                <Search size={10} style={{ color: 'var(--textDim)' }} />
                <input
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  placeholder="Filter notifications..."
                  className="flex-1 bg-transparent border-none outline-none text-[10px]"
                  style={{ color: 'var(--text)' }}
                />
              </div>
            </div>
            <div className="flex-1 overflow-y-auto">
              {filteredNotifications.length === 0 ? (
                <div
                  className="flex items-center justify-center h-full"
                  style={{ color: 'var(--textDim)' }}
                >
                  <span className="text-[10px]">
                    {selectedPaneId ? 'No notifications for this agent' : 'Select an agent'}
                  </span>
                </div>
              ) : (
                filteredNotifications.map((n) => (
                  <div
                    key={n.id}
                    className="px-3 py-2 border-b"
                    style={{ borderColor: 'var(--border)' }}
                  >
                    <div className="flex items-center gap-1">
                      <span className="text-[10px] font-medium" style={{ color: 'var(--text)' }}>
                        {(n as any).title ?? n.message.slice(0, 30)}
                      </span>
                      {(n as any).type && (
                        <span
                          className="text-[8px] px-1 py-px rounded"
                          style={{
                            background: `${(n as any).type === 'error' ? 'var(--error)' : (n as any).type === 'warning' ? 'var(--warning)' : 'var(--accent)'}22`,
                            color:
                              (n as any).type === 'error'
                                ? 'var(--error)'
                                : (n as any).type === 'warning'
                                  ? 'var(--warning)'
                                  : 'var(--accent)',
                          }}
                        >
                          {(n as any).type}
                        </span>
                      )}
                    </div>
                    <p className="text-[9px] mt-0.5" style={{ color: 'var(--textDim)' }}>
                      {n.message}
                    </p>
                  </div>
                ))
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

function StatusRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline gap-2">
      <span className="text-[9px] shrink-0 w-16" style={{ color: 'var(--textDim)' }}>
        {label}
      </span>
      <span className="text-[11px] font-medium truncate" style={{ color: 'var(--text)' }}>
        {value}
      </span>
    </div>
  )
}
