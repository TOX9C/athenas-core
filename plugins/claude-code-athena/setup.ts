#!/usr/bin/env node
import { runPluginSetupCli } from '../shared/cli'

runPluginSetupCli('claude-code').catch((err) => {
  console.error(err instanceof Error ? err.message : String(err))
  process.exit(1)
})
