import type {
  CompanionAgentConversationDetail,
  CompanionAgentMemberSummary,
  CompanionAgentMessage,
} from '../../models/api'
import { parseMarkdown } from '../../components/markdown-view/index'
import { agentTeamService, createAgentClientMessageId } from '../../services/agent-team-service'
import { deviceSelectionStore } from '../../stores/device-selection-store'
import { sessionStore } from '../../stores/session-store'

type MarkdownNode = ReturnType<typeof parseMarkdown>[number]

type MessageView = CompanionAgentMessage & {
  text: string
  markdownNodes: MarkdownNode[]
  senderName: string
  isHuman: boolean
  timeLabel: string
  attachmentLabel: string
}

type MemberView = CompanionAgentMemberSummary & {
  id: string
  initial: string
  selected: boolean
}

function clockTime(value: number): string {
  const date = new Date(value)
  return `${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`
}

function dedupe(messages: MessageView[]): MessageView[] {
  const byId = new Map<string, MessageView>()
  messages.forEach((message) => byId.set(message.id, message))
  return Array.from(byId.values()).sort(
    (left, right) => left.created_at_unix_ms - right.created_at_unix_ms || left.id.localeCompare(right.id),
  )
}

Page({
  data: {
    detail: undefined as CompanionAgentConversationDetail | undefined,
    members: [] as MemberView[],
    messages: [] as MessageView[],
    input: '',
    canSend: false,
    loading: true,
    loadingOlder: false,
    hasMore: false,
    submitting: false,
    selectedMentionCount: 0,
    error: '',
    actionError: '',
    scrollTarget: 'agent-timeline-bottom',
  },

  conversationId: '',
  deviceId: '',
  nextBefore: undefined as string | undefined,
  selectedMentionIds: [] as string[],
  pollTimer: undefined as ReturnType<typeof setInterval> | undefined,
  polling: false,
  pendingClientMessageId: '',
  pendingContent: '',
  pendingMentionKey: '',
  pageVisible: false,
  pageDisposed: false,

  async onLoad(query: Record<string, string | undefined>) {
    this.pageDisposed = false
    await getApp<IAppOption>().authReady
    if (this.pageDisposed) return
    if (!sessionStore.hasToken()) {
      wx.reLaunch({ url: '/pages/bind/index' })
      return
    }
    this.conversationId = decodeURIComponent(query.id ?? '').trim()
    this.deviceId = deviceSelectionStore.get() ?? ''
    if (!this.conversationId || !this.deviceId) {
      this.setData({ loading: false, error: 'Agent 会话参数无效，请返回重试' })
      return
    }
    await this.loadInitial()
  },

  onShow() {
    this.pageVisible = true
    this.startPolling()
  },

  onHide() {
    this.pageVisible = false
    this.stopPolling()
  },

  onUnload() {
    this.pageVisible = false
    this.pageDisposed = true
    this.stopPolling()
  },

  messageView(message: CompanionAgentMessage): MessageView {
    const agent = this.data.detail?.members.find((member) => member.agent.id === message.sender_id)?.agent
    const text = message.content || (message.attachments.length ? '发送了附件' : '')
    return {
      ...message,
      text,
      markdownNodes: parseMarkdown(text),
      senderName: message.sender_kind === 'human' ? '你' : message.sender_kind === 'system' ? '系统' : agent?.name || 'Agent',
      isHuman: message.sender_kind === 'human',
      timeLabel: clockTime(message.created_at_unix_ms),
      attachmentLabel: message.attachments.map((attachment) => attachment.name).join('、'),
    }
  },

  async loadInitial() {
    this.setData({ loading: true, error: '' })
    try {
      const [detail, page] = await Promise.all([
        agentTeamService.conversation(this.deviceId, this.conversationId),
        agentTeamService.messages(this.deviceId, this.conversationId, { limit: 20 }),
      ])
      if (this.pageDisposed) return
      this.setData({ detail })
      const members = detail.members.map((member) => ({
        ...member,
        id: member.agent.id,
        initial: member.agent.name.trim().slice(0, 1).toUpperCase() || 'A',
        selected: false,
      }))
      this.nextBefore = page.next_cursor_message_id
      this.setData({
        members,
        messages: page.messages.map((message) => this.messageView(message)),
        hasMore: page.has_more,
        canSend: detail.conversation.can_send && this.data.input.trim().length > 0,
        loading: false,
        scrollTarget: 'agent-timeline-bottom',
      })
      wx.setNavigationBarTitle({ title: detail.conversation.title || 'Agent 会话' })
      this.startPolling()
    } catch (error) {
      if (this.pageDisposed) return
      this.setData({ loading: false, error: error instanceof Error ? error.message : '读取 Agent 会话失败' })
    }
  },

  async loadOlder() {
    if (!this.nextBefore || this.data.loadingOlder || !this.data.hasMore) return
    this.setData({ loadingOlder: true, actionError: '' })
    try {
      const page = await agentTeamService.messages(this.deviceId, this.conversationId, {
        before: this.nextBefore,
        limit: 20,
      })
      const older = page.messages.map((message) => this.messageView(message))
      this.nextBefore = page.next_cursor_message_id
      const firstCurrent = this.data.messages[0]?.id
      this.setData({
        messages: dedupe([...older, ...this.data.messages]),
        hasMore: page.has_more,
        loadingOlder: false,
        scrollTarget: firstCurrent ? `agent-message-${firstCurrent}` : this.data.scrollTarget,
      })
    } catch (error) {
      this.setData({ loadingOlder: false, actionError: error instanceof Error ? error.message : '加载更早消息失败' })
    }
  },

  onInput(event: WechatMiniprogram.CustomEvent<{ value: string }>) {
    const input = event.detail.value
    this.setData({
      input,
      canSend: Boolean(this.data.detail?.conversation.can_send) && input.trim().length > 0,
      actionError: '',
    })
  },

  toggleMention(event: WechatMiniprogram.TouchEvent) {
    if (this.data.detail?.conversation.kind !== 'project_team') return
    const id = String(event.currentTarget.dataset.id ?? '')
    if (!id) return
    const selected = new Set(this.selectedMentionIds)
    if (selected.has(id)) selected.delete(id)
    else selected.add(id)
    this.selectedMentionIds = Array.from(selected)
    this.setData({
      selectedMentionCount: this.selectedMentionIds.length,
      members: this.data.members.map((member) => ({
        ...member,
        selected: selected.has(member.agent.id),
      })),
    })
  },

  async submitMessage() {
    const content = this.data.input.trim()
    if (!content || !this.data.detail?.conversation.can_send || this.data.submitting) return
    this.setData({ submitting: true, actionError: '' })
    try {
      const mentionKey = [...this.selectedMentionIds].sort().join(',')
      const clientMessageId = this.pendingContent === content
        && this.pendingMentionKey === mentionKey
        && this.pendingClientMessageId
        ? this.pendingClientMessageId
        : createAgentClientMessageId()
      this.pendingClientMessageId = clientMessageId
      this.pendingContent = content
      this.pendingMentionKey = mentionKey
      const response = await agentTeamService.send(
        this.deviceId,
        this.conversationId,
        content,
        this.selectedMentionIds,
        clientMessageId,
      )
      const message = this.messageView(response.message)
      this.selectedMentionIds = []
      this.pendingClientMessageId = ''
      this.pendingContent = ''
      this.pendingMentionKey = ''
      this.setData({
        input: '',
        canSend: false,
        submitting: false,
        selectedMentionCount: 0,
        members: this.data.members.map((member) => ({ ...member, selected: false })),
        messages: dedupe([...this.data.messages, message]),
        scrollTarget: 'agent-timeline-bottom',
      })
      setTimeout(() => void this.pollMessages(), 600)
    } catch (error) {
      this.setData({ submitting: false, actionError: error instanceof Error ? error.message : '发送失败' })
    }
  },

  async pollMessages() {
    if (!this.pageVisible || this.polling || this.data.loading) return
    this.polling = true
    try {
      const lastId = this.data.messages[this.data.messages.length - 1]?.id
      const page = await agentTeamService.messages(this.deviceId, this.conversationId, {
        after: lastId,
        limit: 100,
      })
      if (!this.pageVisible) return
      if (page.messages.length === 0) {
        if (this.data.actionError) this.setData({ actionError: '' })
        return
      }
      this.setData({
        messages: dedupe([
          ...this.data.messages,
          ...page.messages.map((message) => this.messageView(message)),
        ]),
        actionError: '',
        scrollTarget: 'agent-timeline-bottom',
      })
    } catch (error) {
      if (this.pageVisible) this.setData({ actionError: error instanceof Error ? error.message : '刷新消息失败' })
    } finally {
      this.polling = false
    }
  },

  startPolling() {
    if (!this.conversationId || !this.deviceId || this.pollTimer) return
    this.pollTimer = setInterval(() => void this.pollMessages(), 2_500)
  },

  stopPolling() {
    if (this.pollTimer) clearInterval(this.pollTimer)
    this.pollTimer = undefined
  },
})
