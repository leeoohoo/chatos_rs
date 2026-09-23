import { websocketOrigin } from '../config/runtime'
import type { RealtimeEnvelope } from '../models/api'
import { conversationService } from './conversation-service'

type Listener = (event: RealtimeEnvelope) => void

class RealtimeClient {
  private socket?: WechatMiniprogram.SocketTask
  private listeners = new Set<Listener>()
  private conversations = new Set<string>()
  private reconnectTimer?: ReturnType<typeof setTimeout>
  private heartbeatTimer?: ReturnType<typeof setInterval>
  private reconnectAttempt = 0
  private intentionallyClosed = false

  subscribeConversation(id: string, listener: Listener): () => void {
    this.conversations.add(id)
    this.listeners.add(listener)
    this.sendSubscriptions()
    return () => {
      this.listeners.delete(listener)
      this.conversations.delete(id)
      if (this.listeners.size === 0) this.close()
      else this.sendSubscriptions()
    }
  }

  async connect(): Promise<void> {
    if (this.socket) return
    this.intentionallyClosed = false
    let ticket
    try {
      ticket = await conversationService.websocketTicket()
    } catch (error) {
      this.handleDisconnect()
      throw error
    }
    const socket = wx.connectSocket({
      url: `${websocketOrigin()}/api/chatos/realtime/ws?ws_ticket=${encodeURIComponent(ticket.ticket)}`,
      timeout: 10_000,
    })
    this.socket = socket
    socket.onOpen(() => {
      this.reconnectAttempt = 0
      this.startHeartbeat()
      this.sendSubscriptions()
    })
    socket.onMessage((message) => this.handleMessage(message.data))
    socket.onClose(() => this.handleDisconnect())
    socket.onError(() => this.handleDisconnect())
  }

  close(): void {
    this.intentionallyClosed = true
    this.clearTimers()
    this.socket?.close({ code: 1000, reason: 'page closed' })
    this.socket = undefined
  }

  private sendSubscriptions(): void {
    if (!this.socket) return
    const topics = Array.from(this.conversations, (id) => ({ scope: 'conversation', id }))
    this.socket.send({ data: JSON.stringify({ type: 'subscribe', topics }) })
  }

  private handleMessage(data: string | ArrayBuffer): void {
    if (typeof data !== 'string') return
    try {
      const event = JSON.parse(data) as RealtimeEnvelope
      if (event.type !== 'ack' && event.type !== 'pong') {
        this.listeners.forEach((listener) => listener(event))
      }
    } catch {
      // Invalid frames are ignored; the next REST reconciliation remains authoritative.
    }
  }

  private handleDisconnect(): void {
    this.socket = undefined
    this.clearTimers()
    if (this.intentionallyClosed || this.listeners.size === 0) return
    const delay = Math.min(30_000, 1_000 * 2 ** this.reconnectAttempt) + Math.floor(Math.random() * 500)
    this.reconnectAttempt += 1
    this.reconnectTimer = setTimeout(() => void this.connect().catch(() => {}), delay)
  }

  private startHeartbeat(): void {
    this.heartbeatTimer = setInterval(() => {
      this.socket?.send({ data: JSON.stringify({ type: 'ping' }) })
    }, 25_000)
  }

  private clearTimers(): void {
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer)
    if (this.heartbeatTimer) clearInterval(this.heartbeatTimer)
    this.reconnectTimer = undefined
    this.heartbeatTimer = undefined
  }
}

export const realtimeClient = new RealtimeClient()
