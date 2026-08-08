#!/usr/bin/env node

import { readFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')

function extractMainCommands(source) {
  const match = source.match(/generate_handler!\s*\[([\s\S]*?)\]/)
  if (!match) {
    throw new Error('Could not find generate_handler! command list in src-tauri/src/main.rs')
  }

  return extractIdentifiers(match[1], 'main.rs')
}

function extractBuildCommands(source) {
  const match = source.match(/const COMMANDS: &\[&str\] = &\[([\s\S]*?)\];/)
  if (!match) {
    throw new Error('Could not find COMMANDS manifest in src-tauri/build.rs')
  }

  return [...match[1].matchAll(/"([a-zA-Z0-9_]+)"/g)].map(([, command]) => command)
}

function extractIdentifiers(source, fileName) {
  const withoutComments = source.replace(/\/\/.*$/gm, '')
  const identifiers = []

  for (const line of withoutComments.split('\n')) {
    const value = line.trim().replace(/,$/, '')
    if (/^[a-zA-Z_][a-zA-Z0-9_]*$/.test(value)) {
      identifiers.push(value)
    }
  }

  if (identifiers.length === 0) {
    throw new Error(`Could not find commands in ${fileName}`)
  }

  return identifiers
}

function duplicates(values) {
  return [...new Set(values.filter((value, index) => values.indexOf(value) !== index))]
}

const [mainSource, buildSource] = await Promise.all([
  readFile(resolve(root, 'src-tauri/src/main.rs'), 'utf8'),
  readFile(resolve(root, 'src-tauri/build.rs'), 'utf8'),
])

const mainCommands = extractMainCommands(mainSource)
const buildCommands = extractBuildCommands(buildSource)
const mainSet = new Set(mainCommands)
const buildSet = new Set(buildCommands)
const missingFromBuild = mainCommands.filter((command) => !buildSet.has(command))
const missingFromMain = buildCommands.filter((command) => !mainSet.has(command))
const duplicateMain = duplicates(mainCommands)
const duplicateBuild = duplicates(buildCommands)

if (missingFromBuild.length || missingFromMain.length || duplicateMain.length || duplicateBuild.length) {
  console.error('Tauri command registry drift detected.')
  if (missingFromBuild.length) console.error(`Missing from build.rs: ${missingFromBuild.join(', ')}`)
  if (missingFromMain.length) console.error(`Missing from main.rs: ${missingFromMain.join(', ')}`)
  if (duplicateMain.length) console.error(`Duplicate in main.rs: ${duplicateMain.join(', ')}`)
  if (duplicateBuild.length) console.error(`Duplicate in build.rs: ${duplicateBuild.join(', ')}`)
  process.exit(1)
}

console.log(`Tauri command registry is consistent (${mainCommands.length} commands).`)
