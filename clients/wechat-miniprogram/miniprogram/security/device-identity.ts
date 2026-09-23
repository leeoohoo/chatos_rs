import { publicKeyFromSeed, sha512, signDetached } from '../vendor/device-crypto'

const DEVICE_ID_KEY = 'chatos.companion.device-id.v1'
const DEVICE_SEED_KEY = 'chatos.companion.device-seed.v1'

export type CompanionDeviceIdentity = {
  deviceId: string
  publicKey: string
}

function base64Url(bytes: Uint8Array): string {
  const buffer = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer
  return wx.arrayBufferToBase64(buffer).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/g, '')
}

function bytesFromBase64Url(value: string): Uint8Array {
  const normalized = value.replace(/-/g, '+').replace(/_/g, '/')
  const padding = '='.repeat((4 - (normalized.length % 4)) % 4)
  return new Uint8Array(wx.base64ToArrayBuffer(normalized + padding))
}

function utf8(value: string): Uint8Array {
  const output: number[] = []
  for (const symbol of value) {
    const codePoint = symbol.codePointAt(0) ?? 0
    if (codePoint <= 0x7f) output.push(codePoint)
    else if (codePoint <= 0x7ff) {
      output.push(0xc0 | (codePoint >> 6), 0x80 | (codePoint & 0x3f))
    } else if (codePoint <= 0xffff) {
      output.push(
        0xe0 | (codePoint >> 12),
        0x80 | ((codePoint >> 6) & 0x3f),
        0x80 | (codePoint & 0x3f),
      )
    } else {
      output.push(
        0xf0 | (codePoint >> 18),
        0x80 | ((codePoint >> 12) & 0x3f),
        0x80 | ((codePoint >> 6) & 0x3f),
        0x80 | (codePoint & 0x3f),
      )
    }
  }
  return new Uint8Array(output)
}

async function secureRandom(length: number): Promise<Uint8Array> {
  const result = await wx.getRandomValues({ length })
  return new Uint8Array(result.randomValues)
}

class DeviceIdentityStore {
  private cached?: { deviceId: string; seed: Uint8Array }

  async identity(): Promise<CompanionDeviceIdentity> {
    const value = await this.loadOrCreate()
    return {
      deviceId: value.deviceId,
      publicKey: `ed25519:${base64Url(publicKeyFromSeed(value.seed))}`,
    }
  }

  async sign(payload: string): Promise<string> {
    const value = await this.loadOrCreate()
    return base64Url(signDetached(utf8(payload), value.seed))
  }

  bodyDigest(body: string | ArrayBuffer | undefined): string {
    const bytes = body instanceof ArrayBuffer ? new Uint8Array(body) : utf8(body ?? '')
    return base64Url(sha512(bytes))
  }

  async nonce(): Promise<string> {
    return base64Url(await secureRandom(24))
  }

  private async loadOrCreate(): Promise<{ deviceId: string; seed: Uint8Array }> {
    if (this.cached) return this.cached
    const storedDeviceId = wx.getStorageSync<string>(DEVICE_ID_KEY)
    const storedSeed = wx.getStorageSync<string>(DEVICE_SEED_KEY)
    if (typeof storedDeviceId === 'string' && storedDeviceId.trim() && typeof storedSeed === 'string') {
      try {
        const seed = bytesFromBase64Url(storedSeed)
        if (seed.length === 32) {
          this.cached = { deviceId: storedDeviceId, seed }
          return this.cached
        }
      } catch {
        // Invalid local identity is replaced and must be explicitly rebound on the desktop.
      }
    }
    const random = await secureRandom(48)
    const seed = random.slice(0, 32)
    const deviceId = base64Url(random.slice(32))
    wx.setStorageSync(DEVICE_ID_KEY, deviceId)
    wx.setStorageSync(DEVICE_SEED_KEY, base64Url(seed))
    this.cached = { deviceId, seed }
    return this.cached
  }
}

export const deviceIdentityStore = new DeviceIdentityStore()
