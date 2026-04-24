export interface FileNode {
  name: string
  path: string
  isDirectory: boolean
  children?: FileNode[]
}

export interface EditorFile {
  path: string
  content: string
  language: string
  isDirty: boolean
  cursorPosition: { line: number; column: number }
}
