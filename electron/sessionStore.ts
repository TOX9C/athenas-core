import { app } from 'electron'
import { join } from 'path'
import { mkdir, readFile, writeFile, unlink, readdir, stat } from 'fs/promises'
import { existsSync } from 'fs'
import { randomUUID } from 'crypto'

export interface ImageAttachment {
  id: string
  base64: string
  mediaType: 'image/jpeg' | 'image/png' | 'image/gif' | 'image/webp'
  name?: string
}

export interface SessionMessage {
  id: string
  role: 'user' | 'athena'
  content: string
  timestamp: number
  isError?: boolean
  imageRefs?: ImageRef[]
}

export interface ImageRef {
  imageId: string
  mediaType: string
  name?: string
}

export interface ChatSession {
  id: string
  title: string
  createdAt: number
  updatedAt: number
  messages: SessionMessage[]
}

let sessionsDir: string
let imagesDir: string

function getSessionsDir(): string {
  if (!sessionsDir) {
    sessionsDir = join(app.getPath('userData'), 'athena-sessions')
  }
  return sessionsDir
}

function getImagesDir(): string {
  if (!imagesDir) {
    imagesDir = join(app.getPath('userData'), 'athena-images')
  }
  return imagesDir
}

async function ensureDir(dir: string): Promise<void> {
  if (!existsSync(dir)) {
    await mkdir(dir, { recursive: true })
  }
}

function sessionPath(id: string): string {
  return join(getSessionsDir(), `${id}.json`)
}

function imagePath(imageId: string): string {
  return join(getImagesDir(), `${imageId}.bin`)
}

export async function saveImage(
  base64: string,
  mediaType: string,
  name?: string,
): Promise<ImageRef> {
  const imageId = randomUUID()
  await ensureDir(getImagesDir())
  const buffer = Buffer.from(base64, 'base64')
  await writeFile(imagePath(imageId), buffer)
  return { imageId, mediaType, name }
}

export async function loadImage(imageId: string): Promise<string | null> {
  try {
    await ensureDir(getImagesDir())
    const buffer = await readFile(imagePath(imageId))
    return buffer.toString('base64')
  } catch {
    return null
  }
}

export async function deleteImage(imageId: string): Promise<void> {
  try {
    const p = imagePath(imageId)
    if (existsSync(p)) await unlink(p)
  } catch {
    // ignore
  }
}

export async function createSession(title?: string): Promise<ChatSession> {
  await ensureDir(getSessionsDir())
  const session: ChatSession = {
    id: randomUUID(),
    title: title || 'New Chat',
    createdAt: Date.now(),
    updatedAt: Date.now(),
    messages: [],
  }
  await writeFile(sessionPath(session.id), JSON.stringify(session, null, 2), 'utf-8')
  return session
}

export async function getSession(id: string): Promise<ChatSession | null> {
  try {
    await ensureDir(getSessionsDir())
    const data = await readFile(sessionPath(id), 'utf-8')
    const session = JSON.parse(data) as ChatSession
    if (!session.id || !session.title || !Array.isArray(session.messages)) {
      return null
    }
    return session
  } catch {
    return null
  }
}

export async function updateSession(
  id: string,
  updates: Partial<Pick<ChatSession, 'title' | 'messages'>>,
): Promise<ChatSession | null> {
  await ensureDir(getSessionsDir())
  const session = await getSession(id)
  if (!session) return null
  if (updates.title !== undefined) session.title = updates.title
  if (updates.messages !== undefined) session.messages = updates.messages
  session.updatedAt = Date.now()
  await writeFile(sessionPath(id), JSON.stringify(session, null, 2), 'utf-8')
  return session
}

export async function addMessageToSession(
  id: string,
  message: SessionMessage & { images?: { base64: string; mediaType: string; name?: string }[] },
): Promise<ChatSession | null> {
  await ensureDir(getSessionsDir())
  const session = await getSession(id)
  if (!session) return null

  const imageRefs: ImageRef[] = []
  if (message.images && message.images.length > 0) {
    for (const img of message.images) {
      if (img.base64) {
        const ref = await saveImage(img.base64, img.mediaType, img.name)
        imageRefs.push(ref)
      }
    }
  }

  const storedMessage: SessionMessage = {
    id: message.id,
    role: message.role,
    content: message.content,
    timestamp: message.timestamp,
    isError: message.isError,
    imageRefs: imageRefs.length > 0 ? imageRefs : undefined,
  }

  session.messages.push(storedMessage)
  if (session.messages.length === 1 && message.role === 'user') {
    session.title = message.content.slice(0, 80) + (message.content.length > 80 ? '...' : '')
  }
  session.updatedAt = Date.now()
  await writeFile(sessionPath(id), JSON.stringify(session, null, 2), 'utf-8')
  return session
}

export async function deleteSession(id: string): Promise<boolean> {
  try {
    await ensureDir(getSessionsDir())
    const session = await getSession(id)
    if (session) {
      const allRefs = session.messages.flatMap((m) => m.imageRefs ?? [])
      for (const ref of allRefs) {
        await deleteImage(ref.imageId)
      }
    }
    await unlink(sessionPath(id))
    return true
  } catch {
    return false
  }
}

export async function getSessionWithImages(
  id: string,
): Promise<
  (ChatSession & { messages: (SessionMessage & { images?: ImageAttachment[] })[] }) | null
> {
  const session = await getSession(id)
  if (!session) return null

  const messagesWithImages = await Promise.all(
    session.messages.map(async (msg) => {
      if (!msg.imageRefs || msg.imageRefs.length === 0) {
        return { ...msg, images: undefined } as SessionMessage & { images?: ImageAttachment[] }
      }
      const images: ImageAttachment[] = []
      for (const ref of msg.imageRefs) {
        const base64 = await loadImage(ref.imageId)
        if (base64) {
          images.push({
            id: ref.imageId,
            base64,
            mediaType: ref.mediaType as ImageAttachment['mediaType'],
            name: ref.name,
          })
        }
      }
      return { ...msg, images: images.length > 0 ? images : undefined } as SessionMessage & {
        images?: ImageAttachment[]
      }
    }),
  )

  return { ...session, messages: messagesWithImages }
}

export interface SessionListItem {
  id: string
  title: string
  createdAt: number
  updatedAt: number
  messageCount: number
  lastMessagePreview: string
}

export async function listSessions(): Promise<SessionListItem[]> {
  await ensureDir(getSessionsDir())
  const files = await readdir(getSessionsDir())
  const sessions: SessionListItem[] = []

  for (const file of files) {
    if (!file.endsWith('.json')) continue
    try {
      const data = await readFile(join(getSessionsDir(), file), 'utf-8')
      const session: ChatSession = JSON.parse(data)
      if (!session.id || !session.title || !Array.isArray(session.messages)) continue
      const lastMsg = session.messages[session.messages.length - 1]
      sessions.push({
        id: session.id,
        title: session.title,
        createdAt: session.createdAt,
        updatedAt: session.updatedAt,
        messageCount: session.messages.length,
        lastMessagePreview: lastMsg ? lastMsg.content.slice(0, 100) : '',
      })
    } catch {
      // skip corrupted files
    }
  }

  sessions.sort((a, b) => b.updatedAt - a.updatedAt)
  return sessions
}

export async function cleanupOrphanedImages(): Promise<number> {
  await ensureDir(getSessionsDir())
  await ensureDir(getImagesDir())

  const usedImageIds = new Set<string>()
  const sessionFiles = await readdir(getSessionsDir())
  for (const file of sessionFiles) {
    if (!file.endsWith('.json')) continue
    try {
      const data = await readFile(join(getSessionsDir(), file), 'utf-8')
      const session: ChatSession = JSON.parse(data)
      for (const msg of session.messages) {
        if (msg.imageRefs) {
          for (const ref of msg.imageRefs) {
            usedImageIds.add(ref.imageId)
          }
        }
      }
    } catch {
      // skip
    }
  }

  const imageFiles = await readdir(getImagesDir())
  let removed = 0
  for (const file of imageFiles) {
    if (!file.endsWith('.bin')) continue
    const imageId = file.replace('.bin', '')
    if (!usedImageIds.has(imageId)) {
      try {
        await unlink(join(getImagesDir(), file))
        removed++
      } catch {
        // ignore
      }
    }
  }
  return removed
}
