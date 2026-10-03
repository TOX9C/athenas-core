#!/usr/bin/env node
import { runPluginSetupCli } from '../shared/cli'

runPluginSetupCli('opencode').catch((err) => {
  console.error(err instanceof Error ? err.message : String(err))
  process.exit(1)
})
