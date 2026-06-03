// Spec-compliant tools (primary)
export { notify, notifySchema } from './notify.js'
export type { NotifyInput } from './notify.js'

export { statusUpdate, statusUpdateSchema } from './status-update.js'
export type { StatusUpdateInput } from './status-update.js'

export { requestInput, requestInputSchema } from './request-input.js'
export type { RequestInputInput } from './request-input.js'

// Phase 1 tools (athena_ prefixed — backward compatible with assignment requirements)
export { athenaNotify, athenaNotifySchema } from './athena-notify.js'
export type { AthenaNotifyInput } from './athena-notify.js'

export { athenaRequestInput, athenaRequestInputSchema } from './athena-request-input.js'
export type { AthenaRequestInputInput } from './athena-request-input.js'

export { athenaUpdateStatus, athenaUpdateStatusSchema } from './athena-update-status.js'
export type { AthenaUpdateStatusInput } from './athena-update-status.js'

export { athenaReportError, athenaReportErrorSchema } from './athena-report-error.js'
export type { AthenaReportErrorInput } from './athena-report-error.js'

export { athenaReportCompletion, athenaReportCompletionSchema } from './athena-report-completion.js'
export type { AthenaReportCompletionInput } from './athena-report-completion.js'

// Output tools
export { athenaReadOutput, athenaReadOutputSchema } from './athena-read-output.js'
export type { AthenaReadOutputInput } from './athena-read-output.js'

export { athenaStreamOutput, athenaStreamOutputSchema } from './athena-stream-output.js'
export type { AthenaStreamOutputInput } from './athena-stream-output.js'

export { athenaListAgents, athenaListAgentsSchema } from './athena-list-agents.js'
export type { AthenaListAgentsInput } from './athena-list-agents.js'

export { athenaGetOutputSince, athenaGetOutputSinceSchema } from './athena-get-output-since.js'
export type { AthenaGetOutputSinceInput } from './athena-get-output-since.js'

// Search tools
export { searchFiles, searchFilesSchema } from './search-files.js'
export type { SearchFilesInput } from './search-files.js'
