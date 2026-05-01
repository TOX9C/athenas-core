# Plan: Add `fs:getDirectories` IPC handler

## Context

We need to add a new IPC handler to `electron/main.ts` to allow the renderer process to fetch a list of directories in a given path. The underlying function `getDirectories` is already implemented in `electron/fileSystem.ts`.

## Steps

### 1. Update `electron/main.ts`

- Add an IPC handler for `fs:getDirectories`.
- The handler should dynamically import `getDirectories` from `./fileSystem`.
- Handle errors gracefully, returning a structured response containing success status and any error messages if it fails, or the result on success.

**Proposed code snippet for `electron/main.ts`:**

```typescript
ipcMain.handle('fs:getDirectories', async (_event, dirPath: string) => {
  try {
    const { getDirectories } = await import('./fileSystem')
    return await getDirectories(dirPath)
  } catch (err: unknown) {
    return { success: false, error: err instanceof Error ? err.message : String(err) }
  }
})
```

### 2. Verify

- Ensure TypeScript compilation completes successfully.
- Verify through testing (manual or automated) that the renderer process can successfully invoke `window.athena.fs.getDirectories` (assuming it's mapped in preload) and receive the expected array of directory paths.
