import type { ContentBlock } from '../../store/athenaStore'
import { PlanBlockView } from './PlanBlockView'
import { AskUserBlock } from './AskUserBlock'
import { EvaluationBlockView } from './EvaluationBlockView'

interface ContentBlockRendererProps {
  blocks: ContentBlock[]
}

export function ContentBlockRenderer({ blocks }: ContentBlockRendererProps) {
  return (
    <>
      {blocks.map((block, i) => {
        switch (block.type) {
          case 'plan':
            return <PlanBlockView key={`plan-${i}`} block={block} />
          case 'ask_user':
            return <AskUserBlock key={`ask-${block.requestId}`} block={block} />
          case 'evaluation':
            return <EvaluationBlockView key={`eval-${i}`} block={block} />
          default:
            return null
        }
      })}
    </>
  )
}
