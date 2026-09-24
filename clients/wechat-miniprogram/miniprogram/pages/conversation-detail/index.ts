import type {
  AskUserPromptRecord,
  CompanionApproval,
  CompanionTask,
  Conversation,
  ConversationMessage,
} from '../../models/api'
import { parseMarkdown } from '../../components/markdown-view/index'
import { ApiError } from '../../services/api-client'
import { approvalService } from '../../services/approval-service'
import { askUserService } from '../../services/ask-user-service'
import { conversationService } from '../../services/conversation-service'
import { realtimeClient } from '../../services/realtime-client'
import { sessionStore } from '../../stores/session-store'
import { deviceSelectionStore } from '../../stores/device-selection-store'
import { promptView, type AskUserPromptView } from '../../utils/ask-user'
import { messageText } from '../../utils/presentation'

type MarkdownNode = ReturnType<typeof parseMarkdown>[number]

type MessageView = ConversationMessage & {
  text: string
  markdownNodes: MarkdownNode[]
  isUser: boolean
  isAssistant: boolean
  pending?: boolean
  canInspectTask: boolean
}

type TaskProcessView = {
  id: string
  title: string
  detail: string
  detailNodes: MarkdownNode[]
  occurredAt: string
  status: string
  statusLabel: string
}

type TaskView = CompanionTask & {
  statusLabel: string
  resultText: string
  reportText: string
  objectiveNodes: MarkdownNode[]
  descriptionNodes: MarkdownNode[]
  resultNodes: MarkdownNode[]
  reportNodes: MarkdownNode[]
  timeline: TaskProcessView[]
}

type ApprovalView = CompanionApproval & {
  riskLabel: string
  createdLabel: string
  canAccept: boolean
  canAcceptForSession: boolean
  canDecline: boolean
  resolving: boolean
}

type InputEvent = WechatMiniprogram.CustomEvent<{ value: string }>
type DatasetEvent = WechatMiniprogram.TouchEvent

const INTERVENTION_POLL_INTERVAL_MS = 10_000

function messageView(message: ConversationMessage): MessageView {
  const text = messageText(message.content)
  return {
    ...message,
    text,
    markdownNodes: parseMarkdown(text),
    isUser: message.role === 'user',
    isAssistant: message.role === 'assistant',
    canInspectTask:
      message.role === 'assistant' &&
      message.message_mode === 'task_runner_callback' &&
      Boolean(message.task_id),
  }
}

function taskStatusLabel(status?: string): string {
  switch (status?.toLowerCase()) {
    case 'queued': return '排队中'
    case 'running': return '执行中'
    case 'succeeded':
    case 'completed': return '已完成'
    case 'failed': return '失败'
    case 'cancelled':
    case 'canceled': return '已取消'
    case 'blocked': return '等待处理'
    default: return status || '状态未知'
  }
}

function reportText(report: unknown): string {
  if (typeof report === 'string') return report
  if (!report || typeof report !== 'object' || Array.isArray(report)) return ''
  const value = report as Record<string, unknown>
  for (const key of ['content', 'summary', 'output', 'text']) {
    if (typeof value[key] === 'string') return value[key] as string
  }
  return ''
}

function taskTimeline(task: CompanionTask): TaskProcessView[] {
  const log = task.process_log?.trim()
  if (!log) return []
  const entries: Array<{ title: string; detail: string; occurredAt: string }> = []
  let current: { title: string; detail: string; occurredAt: string } | undefined
  const append = () => {
    if (!current) return
    current.detail = current.detail.trim() || '暂无过程说明'
    entries.push(current)
    current = undefined
  }
  log.replace(/\r\n?/g, '\n').split('\n').forEach((line) => {
    const header = line.match(/^\[([^\]]+)]\s*(.*)$/)
    if (header) {
      append()
      current = { occurredAt: header[1].trim(), title: header[2].trim() || '过程记录', detail: '' }
    } else if (line.trim() || current) {
      if (!current) current = { occurredAt: '', title: '过程记录', detail: line }
      else current.detail += current.detail ? `\n${line}` : line
    }
  })
  append()
  return entries.map((entry, index) => {
    const status = index === entries.length - 1 ? (task.status || 'succeeded') : 'succeeded'
    return {
      id: `${task.id}-process-${index}`,
      ...entry,
      detailNodes: parseMarkdown(entry.detail),
      status,
      statusLabel: taskStatusLabel(status),
    }
  })
}

function taskView(task: CompanionTask): TaskView {
  const result = task.result_summary || task.last_run?.result_summary || task.last_run?.error_message || ''
  const report = reportText(task.last_run?.report)
  return {
    ...task,
    statusLabel: taskStatusLabel(task.status),
    resultText: result,
    reportText: report,
    objectiveNodes: parseMarkdown(task.objective || ''),
    descriptionNodes: parseMarkdown(task.description || ''),
    resultNodes: parseMarkdown(result),
    reportNodes: parseMarkdown(report),
    timeline: taskTimeline(task),
  }
}

function approvalView(
  approval: CompanionApproval,
  resolving: boolean,
): ApprovalView {
  const risk = approval.risk.toLowerCase()
  return {
    ...approval,
    riskLabel: risk === 'high' ? '高风险' : risk === 'medium' ? '中风险' : risk === 'low' ? '低风险' : approval.risk,
    createdLabel: approval.created_at ? new Date(approval.created_at).toLocaleTimeString() : '',
    canAccept: approval.available_decisions.includes('accept'),
    canAcceptForSession: approval.available_decisions.includes('acceptForSession'),
    canDecline: approval.available_decisions.includes('decline'),
    resolving,
  }
}

function dedupeMessages(messages: MessageView[]): MessageView[] {
  const seen = new Set<string>()
  return messages.filter((message) => {
    if (seen.has(message.id)) return false
    seen.add(message.id)
    return true
  })
}

function mergeReconciledMessages(
  existing: MessageView[],
  authoritative: MessageView[],
): MessageView[] {
  const authoritativeIds = new Set(authoritative.map((message) => message.id))
  const retained = existing.filter(
    (message) => !message.pending && !authoritativeIds.has(message.id),
  )
  const optimistic = existing.filter(
    (message) => message.pending && !authoritativeIds.has(message.id),
  )
  return dedupeMessages([...retained, ...authoritative, ...optimistic])
}

Page({
  data: {
    conversation: undefined as Conversation | undefined,
    messages: [] as MessageView[],
    prompts: [] as AskUserPromptView[],
    approvals: [] as ApprovalView[],
    taskPanelVisible: false,
    taskPanelTitle: '任务详情',
    taskPanelTasks: [] as TaskView[],
    taskPanelLoading: false,
    taskPanelError: '',
    loadingTaskMessageId: '',
    input: '',
    canSend: false,
    loading: true,
    loadingOlder: false,
    hasMore: false,
    error: '',
    actionError: '',
    submitting: false,
    stopping: false,
    activeTurnId: '',
    scrollTarget: 'timeline-bottom',
    keyboardHeight: 0,
  },

  conversationId: '',
  deviceId: '',
  nextBefore: undefined as string | undefined,
  loadedOlder: false,
  promptRecords: [] as AskUserPromptRecord[],
  promptValues: {} as Record<string, Record<string, string>>,
  promptSelections: {} as Record<string, string[]>,
  submittingPromptIds: [] as string[],
  unsubscribeRealtime: undefined as (() => void) | undefined,
  reconcileTimer: undefined as ReturnType<typeof setTimeout> | undefined,
  postSendTimers: [] as Array<ReturnType<typeof setTimeout>>,
  keyboardListener: undefined as ((result: { height: number }) => void) | undefined,
  interventionTimer: undefined as ReturnType<typeof setInterval> | undefined,
  refreshingPrompts: false,
  refreshingApprovals: false,
  resolvingApprovalIds: [] as string[],
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
    if (!this.conversationId) {
      this.setData({ loading: false, error: '会话参数无效' })
      return
    }
    this.keyboardListener = ({ height }) => this.setData({ keyboardHeight: height })
    wx.onKeyboardHeightChange(this.keyboardListener)
    this.unsubscribeRealtime = realtimeClient.subscribeConversation(
      this.conversationId,
      () => this.scheduleReconcile(),
    )
    void realtimeClient.connect().catch(() => {
      // REST remains authoritative when realtime is temporarily unavailable.
    })
    if (this.conversationId) this.startInterventionPolling()
    void this.loadInitial()
  },

  onShow() {
    if (this.conversationId && !this.data.loading) {
      void realtimeClient.connect().catch(() => {})
      void this.reconcile()
    }
    if (this.conversationId) this.startInterventionPolling()
  },

  onHide() {
    this.stopInterventionPolling()
  },

  onUnload() {
    this.pageDisposed = true
    this.unsubscribeRealtime?.()
    if (this.reconcileTimer) clearTimeout(this.reconcileTimer)
    this.postSendTimers.forEach((timer) => clearTimeout(timer))
    if (this.keyboardListener) wx.offKeyboardHeightChange(this.keyboardListener)
    this.stopInterventionPolling()
  },

  async loadInitial() {
    this.setData({ loading: true, error: '' })
    try {
      const [conversation, history] = await Promise.all([
        conversationService.get(this.conversationId),
        conversationService.history(this.conversationId),
      ])
      if (this.pageDisposed) return
      this.nextBefore = history.next_before
      this.loadedOlder = false
      this.setData({
        conversation,
        messages: history.items.map(messageView),
        hasMore: history.has_more,
        loading: false,
      })
      wx.setNavigationBarTitle({ title: conversation.title || '会话' })
      // onLoad starts the intervention poller, including its immediate refresh.
      // Repeating those relay calls here made the cold path issue two identical
      // approval/prompt queries and increased contention on remote devices.
      await this.refreshRuntime()
      this.scrollToBottom()
    } catch (error) {
      this.setData({ loading: false, error: error instanceof Error ? error.message : '加载会话失败' })
    }
  },

  async loadOlder() {
    if (!this.data.hasMore || this.data.loadingOlder || !this.nextBefore) return
    this.setData({ loadingOlder: true })
    try {
      const history = await conversationService.history(this.conversationId, this.nextBefore)
      this.nextBefore = history.next_before
      this.loadedOlder = true
      this.setData({
        messages: dedupeMessages([...history.items.map(messageView), ...this.data.messages]),
        hasMore: history.has_more,
        loadingOlder: false,
      })
    } catch (error) {
      this.setData({
        loadingOlder: false,
        actionError: error instanceof Error ? error.message : '加载更早消息失败',
      })
    }
  },

  async reconcile() {
    if (!this.conversationId) return
    try {
      const history = await conversationService.history(this.conversationId)
      const serverMessages = history.items.map(messageView)
      if (!this.loadedOlder) this.nextBefore = history.next_before
      this.setData({
        messages: mergeReconciledMessages(this.data.messages, serverMessages),
        hasMore: this.loadedOlder ? this.data.hasMore : history.has_more,
        actionError: '',
      })
      await Promise.all([this.refreshRuntime(), this.refreshPrompts(), this.refreshApprovals()])
      this.scrollToBottom()
    } catch (error) {
      if (error instanceof ApiError && error.statusCode === 401) {
        wx.reLaunch({ url: '/pages/bind/index' })
      }
    }
  },

  async refreshRuntime() {
    const context = await conversationService.runtimeContext(this.conversationId)
    const turnId = context?.turn_id ?? context?.conversation_turn_id ?? ''
    this.setData({ activeTurnId: context?.active_in_runtime === true ? turnId : '' })
  },

  async refreshPrompts(quiet = false) {
    if (!this.conversationId) return
    if (this.refreshingPrompts) return
    this.refreshingPrompts = true
    try {
      this.promptRecords = (await askUserService.list(this.conversationId)).filter(
        (prompt) => prompt.status === 'pending',
      )
      this.rebuildPromptViews()
    } catch (error) {
      if (error instanceof ApiError && error.statusCode === 401) {
        wx.reLaunch({ url: '/pages/bind/index' })
      } else if (!quiet) {
        throw error
      }
    } finally {
      this.refreshingPrompts = false
    }
  },

  async refreshApprovals() {
    if (this.refreshingApprovals) return
    if (!this.deviceId) {
      this.setData({ approvals: [] })
      return
    }
    this.refreshingApprovals = true
    try {
      const approvals = await approvalService.list(this.deviceId)
      this.setData({
        approvals: approvals.map((approval) =>
          approvalView(approval, this.resolvingApprovalIds.includes(approval.id))),
      })
    } catch (error) {
      if (error instanceof ApiError && error.statusCode === 401) {
        wx.reLaunch({ url: '/pages/bind/index' })
      }
    } finally {
      this.refreshingApprovals = false
    }
  },

  startInterventionPolling() {
    this.stopInterventionPolling()
    if (!this.conversationId || this.pageDisposed || !this.isCurrentPage()) return
    void this.refreshPrompts(true)
    if (this.deviceId) void this.refreshApprovals()
    this.interventionTimer = setInterval(() => {
      if (this.pageDisposed || !this.isCurrentPage()) {
        this.stopInterventionPolling()
        return
      }
      void this.refreshPrompts(true)
      if (this.deviceId) void this.refreshApprovals()
    }, INTERVENTION_POLL_INTERVAL_MS)
  },

  stopInterventionPolling() {
    if (this.interventionTimer) clearInterval(this.interventionTimer)
    this.interventionTimer = undefined
  },

  isCurrentPage(): boolean {
    const pages = getCurrentPages()
    return pages.length > 0 && pages[pages.length - 1] === this
  },

  async openTaskDetail(event: DatasetEvent) {
    const messageId = String(event.currentTarget.dataset.messageId ?? '')
    const taskId = String(event.currentTarget.dataset.taskId ?? '')
    if (!messageId || this.data.loadingTaskMessageId) return
    this.setData({
      loadingTaskMessageId: messageId,
      actionError: '',
      taskPanelVisible: true,
      taskPanelTitle: '任务详情',
      taskPanelTasks: [],
      taskPanelLoading: true,
      taskPanelError: '',
    })
    try {
      const response = await conversationService.tasks(messageId, taskId || undefined)
      if (!response.items.length) {
        this.setData({ taskPanelError: '这条消息没有任务详情' })
        return
      }
      this.setData({
        taskPanelTitle: response.items.length > 1 ? `任务详情（${response.items.length}）` : '任务详情',
        taskPanelTasks: response.items.map(taskView),
      })
    } catch (error) {
      this.setData({ taskPanelError: error instanceof Error ? error.message : '读取任务详情失败' })
    } finally {
      this.setData({ loadingTaskMessageId: '', taskPanelLoading: false })
    }
  },

  closeTaskPanel() {
    this.setData({ taskPanelVisible: false, taskPanelError: '' })
  },

  stopEvent() {},

  resolveApproval(event: DatasetEvent) {
    const approvalId = String(event.currentTarget.dataset.approvalId ?? '')
    const decision = String(event.currentTarget.dataset.decision ?? '') as 'accept' | 'acceptForSession' | 'decline'
    if (!approvalId || this.resolvingApprovalIds.includes(approvalId)) return
    const copy = decision === 'decline'
      ? { title: '拒绝这次操作？', content: '电脑上的当前任务会收到拒绝结果。', confirmText: '拒绝', confirmColor: '#FF3B30' }
      : decision === 'acceptForSession'
        ? { title: '允许当前会话？', content: '同一执行会话中的同类操作可能不再逐次询问。', confirmText: '允许会话', confirmColor: '#007AFF' }
        : { title: '批准这次操作？', content: '只批准当前显示的这一项本机操作。', confirmText: '批准', confirmColor: '#007AFF' }
    wx.showModal({
      ...copy,
      success: (result) => {
        if (result.confirm) void this.performResolveApproval(approvalId, decision)
      },
    })
  },

  async performResolveApproval(
    approvalId: string,
    decision: 'accept' | 'acceptForSession' | 'decline',
  ) {
    if (!this.deviceId) return
    this.resolvingApprovalIds.push(approvalId)
    await this.refreshApprovals()
    try {
      await approvalService.resolve(this.deviceId, approvalId, decision)
      await this.refreshApprovals()
      this.schedulePostSendReconciliation()
    } catch (error) {
      this.setData({ actionError: error instanceof Error ? error.message : '处理审批失败' })
    } finally {
      this.resolvingApprovalIds = this.resolvingApprovalIds.filter((id) => id !== approvalId)
      await this.refreshApprovals()
    }
  },

  rebuildPromptViews() {
    this.setData({
      prompts: this.promptRecords.map((record) => promptView(
        record,
        this.promptValues[record.id],
        this.promptSelections[record.id],
        this.submittingPromptIds.includes(record.id),
      )),
    })
  },

  onInput(event: InputEvent) {
    this.setData({ input: event.detail.value, canSend: Boolean(event.detail.value.trim()) })
  },

  async submitMessage() {
    const content = this.data.input.trim()
    if (!content || this.data.submitting) return
    if (content.length > 20_000) {
      this.setData({ actionError: '单条消息最多 20,000 个字符' })
      return
    }
    const optimistic: MessageView = {
      id: `pending-${Date.now()}`,
      role: 'user',
      content,
      text: content,
      markdownNodes: parseMarkdown(content),
      isUser: true,
      isAssistant: false,
      pending: true,
      canInspectTask: false,
    }
    const activeTurnId = this.data.activeTurnId
    this.setData({
      input: '',
      canSend: false,
      submitting: true,
      actionError: '',
      messages: [...this.data.messages, optimistic],
    })
    this.scrollToBottom()
    try {
      if (activeTurnId) {
        try {
          const response = await conversationService.guidance(
            this.conversationId,
            activeTurnId,
            content,
          )
          this.adoptOptimisticMessageId(optimistic.id, response.message_id)
        } catch (error) {
          if (!(error instanceof ApiError) || error.statusCode !== 409) throw error
          const response = await conversationService.send(this.conversationId, content)
          this.adoptOptimisticMessageId(optimistic.id, response.user_message_id)
          this.setData({ activeTurnId: response.turn_id ?? '' })
        }
      } else {
        const response = await conversationService.send(this.conversationId, content)
        this.adoptOptimisticMessageId(optimistic.id, response.user_message_id)
        this.setData({ activeTurnId: response.turn_id ?? '' })
      }
      this.schedulePostSendReconciliation()
    } catch (error) {
      this.setData({
        input: content,
        canSend: true,
        messages: this.data.messages.filter((message) => message.id !== optimistic.id),
        actionError: error instanceof Error ? error.message : '消息发送失败',
      })
    } finally {
      this.setData({ submitting: false })
    }
  },

  async stopTurn() {
    if (!this.data.activeTurnId || this.data.stopping) return
    this.setData({ stopping: true, actionError: '' })
    try {
      await conversationService.stop(this.conversationId, this.data.activeTurnId)
      this.setData({ activeTurnId: '' })
      this.schedulePostSendReconciliation()
    } catch (error) {
      this.setData({ actionError: error instanceof Error ? error.message : '停止失败' })
    } finally {
      this.setData({ stopping: false })
    }
  },

  onPromptInput(event: InputEvent) {
    const promptId = String(event.currentTarget.dataset.promptId ?? '')
    const fieldKey = String(event.currentTarget.dataset.fieldKey ?? '')
    if (!promptId || !fieldKey) return
    this.promptValues[promptId] = {
      ...(this.promptValues[promptId] ?? {}),
      [fieldKey]: event.detail.value,
    }
    this.rebuildPromptViews()
  },

  toggleChoice(event: DatasetEvent) {
    const promptId = String(event.currentTarget.dataset.promptId ?? '')
    const value = String(event.currentTarget.dataset.value ?? '')
    const prompt = this.data.prompts.find((item) => item.id === promptId)
    if (!prompt?.choice || !value || this.submittingPromptIds.includes(promptId)) return
    const selected = new Set(this.promptSelections[promptId] ?? prompt.selection)
    if (prompt.choice.multiple) {
      if (selected.has(value)) selected.delete(value)
      else if (selected.size < prompt.choice.maximum) selected.add(value)
    } else {
      selected.clear()
      selected.add(value)
    }
    this.promptSelections[promptId] = Array.from(selected)
    this.rebuildPromptViews()
  },

  async submitPrompt(event: DatasetEvent) {
    const promptId = String(event.currentTarget.dataset.promptId ?? '')
    const prompt = this.data.prompts.find((item) => item.id === promptId)
    if (!prompt?.valid || this.submittingPromptIds.includes(promptId)) return
    this.submittingPromptIds.push(promptId)
    this.rebuildPromptViews()
    try {
      const selection = prompt.choice
        ? prompt.choice.multiple ? prompt.selection : prompt.selection[0]
        : undefined
      await askUserService.submit(promptId, this.conversationId, prompt.values, selection)
      delete this.promptValues[promptId]
      delete this.promptSelections[promptId]
      await this.refreshPrompts()
    } catch (error) {
      this.setData({ actionError: error instanceof Error ? error.message : '提交答复失败' })
    } finally {
      this.submittingPromptIds = this.submittingPromptIds.filter((id) => id !== promptId)
      this.rebuildPromptViews()
    }
  },

  cancelPrompt(event: DatasetEvent) {
    const promptId = String(event.currentTarget.dataset.promptId ?? '')
    if (!promptId || this.submittingPromptIds.includes(promptId)) return
    wx.showModal({
      title: '取消这个请求？',
      content: '电脑上的当前任务会收到取消结果。',
      confirmText: '取消请求',
      confirmColor: '#FF3B30',
      success: (result) => {
        if (result.confirm) void this.performCancelPrompt(promptId)
      },
    })
  },

  async performCancelPrompt(promptId: string) {
    this.submittingPromptIds.push(promptId)
    this.rebuildPromptViews()
    try {
      await askUserService.cancel(promptId, this.conversationId)
      await this.refreshPrompts()
    } catch (error) {
      this.setData({ actionError: error instanceof Error ? error.message : '取消请求失败' })
    } finally {
      this.submittingPromptIds = this.submittingPromptIds.filter((id) => id !== promptId)
      this.rebuildPromptViews()
    }
  },

  scheduleReconcile() {
    if (this.reconcileTimer) clearTimeout(this.reconcileTimer)
    this.reconcileTimer = setTimeout(() => void this.reconcile(), 350)
  },

  adoptOptimisticMessageId(temporaryId: string, authoritativeId?: string) {
    if (!authoritativeId) return
    this.setData({
      messages: this.data.messages.map((message) =>
        message.id === temporaryId ? { ...message, id: authoritativeId } : message,
      ),
    })
  },

  schedulePostSendReconciliation() {
    this.postSendTimers.forEach((timer) => clearTimeout(timer))
    this.postSendTimers = [500, 1_500, 4_000].map((delay) =>
      setTimeout(() => void this.reconcile(), delay),
    )
  },

  scrollToBottom() {
    this.setData({ scrollTarget: '' }, () => {
      this.setData({ scrollTarget: 'timeline-bottom' })
    })
  },
})
