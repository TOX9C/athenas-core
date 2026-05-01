import { getAgentColor } from '../../utils/agentCommands'
import type { Plugin } from '../../types/notification'
import { Power, Settings, AlertCircle, Loader, CheckCircle } from 'lucide-react'

const STATUS_CONFIG: Record<string, { icon: typeof CheckCircle; color: string; label: string }> = {
  active: { icon: CheckCircle, color: 'var(--success)', label: 'Active' },
  inactive: { icon: Power, color: 'var(--textDim)', label: 'Inactive' },
  error: { icon: AlertCircle, color: 'var(--error)', label: 'Error' },
  installing: { icon: Loader, color: 'var(--accent)', label: 'Installing' },
  updating: { icon: Loader, color: 'var(--warning)', label: 'Updating' },
}

export function PluginCard({
  plugin,
  onToggle,
  onConfigure,
}: {
  plugin: Plugin
  onToggle: (id: string) => void
  onConfigure?: (id: string) => void
}) {
  const statusCfg = STATUS_CONFIG[plugin.status] ?? STATUS_CONFIG.inactive
  const StatusIcon = statusCfg.icon

  return (
    <div
      className="rounded-lg p-3 flex flex-col gap-2.5"
      style={{
        background: 'var(--bgSecondary)',
        border: `1px solid ${plugin.status === 'error' ? 'var(--error)' : 'var(--border)'}`,
        opacity: plugin.enabled ? 1 : 0.6,
      }}
    >
      <div className="flex items-start justify-between">
        <div className="flex items-center gap-2">
          {plugin.icon ? (
            <span className="text-base">{plugin.icon}</span>
          ) : (
            <div
              className="w-6 h-6 rounded flex items-center justify-center text-[10px] font-bold"
              style={{ background: 'var(--bgTertiary)', color: 'var(--textMuted)' }}
            >
              {plugin.name.charAt(0).toUpperCase()}
            </div>
          )}
          <div>
            <span className="text-[11px] font-semibold block" style={{ color: 'var(--text)' }}>
              {plugin.name}
            </span>
            <span className="text-[9px]" style={{ color: 'var(--textDim)' }}>
              v{plugin.version} · {plugin.author}
            </span>
          </div>
        </div>
        <button
          onClick={() => onToggle(plugin.id)}
          className="p-1 rounded transition-colors"
          style={{
            background: plugin.enabled ? 'var(--success)' : 'var(--bgTertiary)',
            color: plugin.enabled ? '#fff' : 'var(--textDim)',
          }}
          title={plugin.enabled ? 'Disable' : 'Enable'}
        >
          <Power size={10} />
        </button>
      </div>

      <p className="text-[10px] leading-relaxed" style={{ color: 'var(--textMuted)' }}>
        {plugin.description}
      </p>

      <div className="flex items-center justify-between">
        <div className="flex items-center gap-1.5">
          <StatusIcon
            size={10}
            style={{ color: statusCfg.color }}
            className={
              plugin.status === 'installing' || plugin.status === 'updating' ? 'animate-spin' : ''
            }
          />
          <span className="text-[9px]" style={{ color: statusCfg.color }}>
            {statusCfg.label}
          </span>
          {plugin.agentCount > 0 && (
            <span className="text-[9px] ml-1" style={{ color: 'var(--textDim)' }}>
              · {plugin.agentCount} agent{plugin.agentCount !== 1 ? 's' : ''}
            </span>
          )}
        </div>

        {plugin.capabilities.length > 0 && (
          <div className="flex items-center gap-1">
            {plugin.capabilities.slice(0, 3).map((cap) => (
              <span
                key={cap}
                className="text-[8px] px-1 py-px rounded"
                style={{ background: 'var(--bgTertiary)', color: 'var(--textDim)' }}
              >
                {cap}
              </span>
            ))}
            {plugin.capabilities.length > 3 && (
              <span className="text-[8px]" style={{ color: 'var(--textDim)' }}>
                +{plugin.capabilities.length - 3}
              </span>
            )}
          </div>
        )}
      </div>

      {plugin.error && (
        <div
          className="text-[9px] px-2 py-1 rounded"
          style={{ background: 'var(--error)', color: '#fff', opacity: 0.9 }}
        >
          {plugin.error}
        </div>
      )}

      {onConfigure && plugin.config && (
        <button
          onClick={() => onConfigure(plugin.id)}
          className="flex items-center gap-1 text-[9px] transition-colors self-start"
          style={{ color: 'var(--accent)' }}
        >
          <Settings size={9} />
          Configure
        </button>
      )}
    </div>
  )
}
