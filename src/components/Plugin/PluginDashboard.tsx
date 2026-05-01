import { useState } from 'react'
import { Puzzle, Search, RefreshCw, Plus, Filter } from 'lucide-react'
import { PluginCard } from './PluginCard'
import { useNotificationStore } from '../../store/notificationStore'
import type { Plugin, PluginStatus } from '../../types/notification'

type StatusFilter = 'all' | PluginStatus

export function PluginDashboard() {
  const { plugins, togglePlugin, updatePlugin } = useNotificationStore()
  const [search, setSearch] = useState('')
  const [statusFilter, setStatusFilter] = useState<StatusFilter>('all')

  const activeCount = plugins.filter((p) => p.enabled).length
  const errorCount = plugins.filter((p) => p.status === 'error').length

  const filtered = plugins.filter((p) => {
    if (statusFilter !== 'all' && p.status !== statusFilter) return false
    if (search) {
      const q = search.toLowerCase()
      return (
        p.name.toLowerCase().includes(q) ||
        p.description.toLowerCase().includes(q) ||
        p.author.toLowerCase().includes(q)
      )
    }
    return true
  })

  return (
    <div className="flex flex-col h-full">
      <div
        className="flex items-center justify-between px-4 py-3 border-b"
        style={{ borderColor: 'var(--border)' }}
      >
        <div className="flex items-center gap-2">
          <Puzzle size={14} style={{ color: 'var(--accent)' }} />
          <span className="text-xs font-semibold" style={{ color: 'var(--text)' }}>
            Plugins
          </span>
          <span
            className="text-[9px] px-1.5 py-0.5 rounded-full"
            style={{ background: 'var(--bgTertiary)', color: 'var(--textDim)' }}
          >
            {plugins.length}
          </span>
          <span
            className="text-[9px] px-1.5 py-0.5 rounded-full"
            style={{ background: 'var(--success)', color: '#fff', opacity: 0.8 }}
          >
            {activeCount} active
          </span>
          {errorCount > 0 && (
            <span
              className="text-[9px] px-1.5 py-0.5 rounded-full"
              style={{ background: 'var(--error)', color: '#fff' }}
            >
              {errorCount} error{errorCount !== 1 ? 's' : ''}
            </span>
          )}
        </div>
        <button
          onClick={() => {
            window.athena?.plugin?.list?.().then((registry: Record<string, any>) => {
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
          }}
          className="p-1 rounded hover:bg-white/10 transition-colors"
          title="Refresh plugins"
        >
          <RefreshCw size={12} style={{ color: 'var(--textDim)' }} />
        </button>
      </div>

      <div
        className="flex items-center gap-2 px-4 py-2 border-b"
        style={{ borderColor: 'var(--border)' }}
      >
        <div
          className="flex items-center gap-1.5 flex-1 px-2 py-1.5 rounded-md"
          style={{ background: 'var(--bgTertiary)', border: '1px solid var(--border)' }}
        >
          <Search size={11} style={{ color: 'var(--textDim)' }} />
          <input
            type="text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search plugins..."
            className="flex-1 bg-transparent outline-none text-[11px]"
            style={{ color: 'var(--text)' }}
          />
        </div>
        <div className="flex items-center gap-0.5">
          {(['all', 'active', 'error', 'inactive'] as StatusFilter[]).map((s) => {
            const count =
              s === 'all' ? plugins.length : plugins.filter((p) => p.status === s).length
            if (s !== 'all' && count === 0) return null
            return (
              <button
                key={s}
                onClick={() => setStatusFilter(s)}
                className="px-2 py-1 rounded text-[9px] font-medium transition-colors capitalize"
                style={{
                  background: statusFilter === s ? 'var(--accent)' : 'transparent',
                  color: statusFilter === s ? '#fff' : 'var(--textDim)',
                }}
              >
                {s}
              </button>
            )
          })}
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-3">
        {filtered.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-12 gap-3">
            <Puzzle size={28} style={{ color: 'var(--textDim)', opacity: 0.3 }} />
            <span className="text-[11px]" style={{ color: 'var(--textDim)' }}>
              {plugins.length === 0 ? 'No plugins installed' : 'No matching plugins'}
            </span>
          </div>
        ) : (
          <div
            className="grid gap-2"
            style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))' }}
          >
            {filtered.map((plugin) => (
              <PluginCard
                key={plugin.id}
                plugin={plugin}
                onToggle={togglePlugin}
                onConfigure={(id) => {
                  const p = plugins.find((pl) => pl.id === id)
                  if (p?.config) console.log('Configure plugin:', id, p.config)
                }}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
