import type { AthenaMessage, ImageAttachment } from '../../store/athenaStore'
import { ContentBlockRenderer } from './ContentBlockRenderer'
import { User, Brain } from 'lucide-react'

function MessageImage({ image }: { image: ImageAttachment }) {
  const src = image.base64 ? `data:${image.mediaType};base64,${image.base64}` : undefined

  if (!src) return null

  return (
    <div className="mt-1.5 mb-1">
      <img
        src={src}
        alt={image.name || 'Attached image'}
        className="max-w-full rounded cursor-pointer hover:opacity-90 transition-opacity"
        style={{
          maxHeight: '180px',
          objectFit: 'contain',
        }}
        loading="lazy"
      />
      {image.name && (
        <span className="block text-[9px] mt-0.5" style={{ color: 'var(--textDim)' }}>
          {image.name}
        </span>
      )}
    </div>
  )
}

interface AthenaChatMessageProps {
  message: AthenaMessage
}

export function AthenaChatMessage({ message }: AthenaChatMessageProps) {
  const isUser = message.role === 'user'
  const time = new Date(message.timestamp).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  })

  return (
    <div
      className="flex gap-2.5 py-2.5 px-3 w-full"
      style={{
        background: isUser ? 'transparent' : 'color-mix(in srgb, var(--accent) 3%, transparent)',
        borderBottom: '1px solid color-mix(in srgb, var(--border) 40%, transparent)',
      }}
    >
      <div className="shrink-0 mt-0.5">
        {isUser ? (
          <div
            className="w-5 h-5 rounded flex items-center justify-center"
            style={{ background: 'var(--bgTertiary)' }}
          >
            <User size={11} style={{ color: 'var(--textDim)' }} />
          </div>
        ) : (
          <div
            className="w-5 h-5 rounded flex items-center justify-center"
            style={{ background: 'var(--accent)' }}
          >
            <Brain size={11} style={{ color: '#fff' }} />
          </div>
        )}
      </div>

      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-1.5 mb-0.5">
          <span
            className="text-[11px] font-medium"
            style={{ color: isUser ? 'var(--textMuted)' : 'var(--accent)' }}
          >
            {isUser ? 'You' : 'Athena'}
          </span>
          <span className="text-[9px]" style={{ color: 'var(--textDim)', opacity: 0.6 }}>
            {time}
          </span>
        </div>

        {message.images && message.images.length > 0 && (
          <div className="flex flex-wrap gap-1.5">
            {message.images.map((img) => (
              <MessageImage key={img.id} image={img} />
            ))}
          </div>
        )}

        <div
          className="text-xs leading-relaxed whitespace-pre-wrap break-words"
          style={{ color: message.isError ? 'var(--error)' : 'var(--text)' }}
        >
          {message.content}
        </div>

        {message.blocks && message.blocks.length > 0 && (
          <ContentBlockRenderer blocks={message.blocks} />
        )}
      </div>
    </div>
  )
}
