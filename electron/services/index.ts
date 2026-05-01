export {
  initPluginManager,
  registerPluginIpcHandlers,
  getPluginRegistry,
  getEnabledPlugins,
  getPluginById,
  setPluginStatus,
} from './plugin-manager'
export type {
  PluginManifest as PluginManagerManifest,
  PluginPermission,
  PluginStatus,
  PluginEntry,
} from './plugin-manager'

export {
  initAgentComms,
  getCommsToken,
  getAgentSessions,
  broadcastToAgents,
  sendToAgent,
  respondToInputRequest,
  shutdownAgentComms,
} from './agent-comms'
export type { AgentMessageType, AgentMessage, AgentSession } from './agent-comms'

export {
  initNotificationService,
  pushNotification,
  getNotificationHistory,
  getUnreadCount,
} from './notification-service'
export type { NotificationType, NotificationEvent } from './notification-service'

export {
  initOutputCapture,
  onPtySpawn,
  onPtyData,
  onPtyExit,
  captureStderr,
  shutdownOutputCapture,
} from './output-capture'

export {
  initOutputBufferService,
  appendOutput,
  registerPane,
  unregisterPane,
  getOutput,
  getOutputSince,
  getAgentList,
  subscribeToPane,
  getPaneBufferInfo,
  clearPaneBuffer,
  shutdownOutputBufferService,
} from './output-buffer-service'
export type { OutputLine } from './output-buffer-service'
