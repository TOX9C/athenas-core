const VALID_IMAGE_TYPES = new Set([
  'image/png',
  'image/jpeg',
  'image/jpg',
  'image/gif',
  'image/webp',
  'image/bmp',
])

const VALID_IMAGE_EXTENSIONS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp'])

const DEFAULT_MAX_WIDTH = 2048
const DEFAULT_MAX_HEIGHT = 2048
const DEFAULT_QUALITY = 0.8
const DEFAULT_MAX_SIZE_MB = 10

export interface ImageDimensions {
  width: number
  height: number
}

export interface ValidationResult {
  valid: boolean
  error?: string
}

export function isImageFile(file: File): boolean {
  if (VALID_IMAGE_TYPES.has(file.type)) return true
  const ext = file.name.split('.').pop()?.toLowerCase() ?? ''
  return VALID_IMAGE_EXTENSIONS.has(ext)
}

export function validateImage(
  file: File,
  maxSizeMB: number = DEFAULT_MAX_SIZE_MB,
): ValidationResult {
  if (!isImageFile(file)) {
    return {
      valid: false,
      error: `Unsupported image type: ${file.type || 'unknown'}. Supported: PNG, JPG, GIF, WebP, BMP`,
    }
  }
  if (file.size > maxSizeMB * 1024 * 1024) {
    return {
      valid: false,
      error: `Image is ${(file.size / (1024 * 1024)).toFixed(1)}MB, exceeding the ${maxSizeMB}MB limit`,
    }
  }
  return { valid: true }
}

export function getImageDimensions(file: File): Promise<ImageDimensions> {
  return new Promise((resolve, reject) => {
    const img = new Image()
    const url = URL.createObjectURL(file)
    img.onload = () => {
      resolve({ width: img.naturalWidth, height: img.naturalHeight })
      URL.revokeObjectURL(url)
    }
    img.onerror = () => {
      URL.revokeObjectURL(url)
      reject(new Error(`Failed to load image: ${file.name}`))
    }
    img.src = url
  })
}

export function compressImage(
  file: File,
  maxWidth: number = DEFAULT_MAX_WIDTH,
  maxHeight: number = DEFAULT_MAX_HEIGHT,
  quality: number = DEFAULT_QUALITY,
): Promise<Blob> {
  return new Promise((resolve, reject) => {
    const img = new Image()
    const url = URL.createObjectURL(file)
    img.onload = () => {
      URL.revokeObjectURL(url)
      let { naturalWidth: w, naturalHeight: h } = img

      if (w > maxWidth || h > maxHeight) {
        const ratio = Math.min(maxWidth / w, maxHeight / h)
        w = Math.round(w * ratio)
        h = Math.round(h * ratio)
      }

      const canvas = document.createElement('canvas')
      canvas.width = w
      canvas.height = h
      const ctx = canvas.getContext('2d')
      if (!ctx) {
        reject(new Error('Failed to get canvas 2d context'))
        return
      }
      ctx.drawImage(img, 0, 0, w, h)

      const outputType = file.type === 'image/png' ? 'image/png' : 'image/jpeg'
      const outputQuality = outputType === 'image/png' ? undefined : quality

      canvas.toBlob(
        (blob) => {
          if (blob) {
            resolve(blob)
          } else {
            reject(new Error(`Failed to compress image: ${file.name}`))
          }
        },
        outputType,
        outputQuality,
      )
    }
    img.onerror = () => {
      URL.revokeObjectURL(url)
      reject(new Error(`Failed to load image for compression: ${file.name}`))
    }
    img.src = url
  })
}

export function imageToBase64(file: File | Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => {
      const result = reader.result
      if (typeof result === 'string') {
        resolve(result)
      } else {
        reject(new Error('FileReader did not return a string'))
      }
    }
    reader.onerror = () => {
      reject(
        new Error(`Failed to read file as base64: ${file instanceof File ? file.name : 'Blob'}`),
      )
    }
    reader.readAsDataURL(file)
  })
}

export async function processImageForSending(
  file: File,
  maxWidth: number = DEFAULT_MAX_WIDTH,
  maxHeight: number = DEFAULT_MAX_HEIGHT,
  quality: number = DEFAULT_QUALITY,
  maxSizeMB: number = DEFAULT_MAX_SIZE_MB,
): Promise<string> {
  const validation = validateImage(file, maxSizeMB)
  if (!validation.valid) {
    throw new Error(validation.error)
  }

  const compressed = await compressImage(file, maxWidth, maxHeight, quality)
  return imageToBase64(compressed)
}

export function extractBase64Data(dataUri: string): { mediaType: string; base64: string } {
  const match = dataUri.match(/^data:([^;]+);base64,(.+)$/)
  if (!match) {
    throw new Error('Invalid data URI format')
  }
  return { mediaType: match[1], base64: match[2] }
}
