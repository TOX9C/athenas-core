import { CommandBlock } from './CommandBlock'
import type { CommandBlock as CommandBlockType } from '../../types/terminal'

interface CommandBlockListProps {
  blocks: CommandBlockType[]
  onToggle: (id: string) => void
  onRerun?: (command: string) => void
}

export function CommandBlockList({ blocks, onToggle, onRerun }: CommandBlockListProps) {
  if (blocks.length === 0) return null

  return (
    <div className="flex flex-col gap-1 p-1.5 overflow-y-auto">
      {blocks.map((block) => (
        <CommandBlock
          key={block.id}
          id={block.id}
          command={block.command}
          output={block.output}
          exitCode={block.exitCode}
          startedAt={block.startedAt}
          finishedAt={block.finishedAt}
          collapsed={block.collapsed}
          onToggle={() => onToggle(block.id)}
          onRerun={onRerun ? () => onRerun(block.command) : undefined}
          onCopyCommand={() => navigator.clipboard.writeText(block.command)}
          onCopyOutput={() => navigator.clipboard.writeText(block.output)}
        />
      ))}
    </div>
  )
}
