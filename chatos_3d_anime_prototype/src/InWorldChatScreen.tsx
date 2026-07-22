import {
  Bot,
  BrainCircuit,
  Check,
  Copy,
  FileText,
  FolderOpen,
  Image as ImageIcon,
  ListChecks,
  LoaderCircle,
  MessageSquareText,
  Paperclip,
  RefreshCw,
  Search,
  Send,
  Square,
  Trash2,
  UserPlus,
  UserRound,
  UsersRound,
  Wifi,
  WifiOff,
  X,
} from 'lucide-react';
import { FormEvent, useEffect, useMemo, useRef, useState } from 'react';
import { formatFileSize, validateAttachmentFiles } from './chatAttachments';
import { MarkdownMessage } from './MarkdownMessage';
import type {
  ChatMessage,
  ChatAgentOption,
  ChatContact,
  ChatModelOption,
  ChatRuntimeSettings,
  DemoProject,
} from './types';

interface PendingFile {
  id: string;
  file: File;
  previewUrl?: string;
}

interface InWorldChatScreenProps {
  messages: ChatMessage[];
  contacts: ChatContact[];
  accountContacts: ChatContact[];
  availableAgents: ChatAgentOption[];
  models: ChatModelOption[];
  projects: DemoProject[];
  runtimeSettings: ChatRuntimeSettings;
  activeProjectId: string | null;
  activeContactId: string | null;
  thinking: boolean;
  isStopping: boolean;
  loadingMessages: boolean;
  hasMoreMessages: boolean;
  sessionBusy: boolean;
  live: boolean;
  webSocketStatus: string;
  error: string | null;
  conversationId: string | null;
  conversationTitle: string | null;
  onSend: (content: string, files: File[]) => void | Promise<void>;
  onStop: () => void | Promise<void>;
  onSelectContact: (contactId: string) => void | Promise<void>;
  onAddContact: (agentId: string) => void | Promise<unknown>;
  onDeleteContact: (contactId: string) => void | Promise<void>;
  onAssignProjectContact: (contactId: string) => void | Promise<void>;
  onRemoveProjectContact: (contactId: string) => void | Promise<void>;
  onRefresh: () => void | Promise<void>;
  onLoadMore: () => void | Promise<void>;
  onRuntimeChange: (patch: Partial<ChatRuntimeSettings>) => void | Promise<void>;
  onProjectChange: (projectId: string | null) => void | Promise<void>;
}

const MAX_MESSAGE_LENGTH = 12000;

function MessageCopyButton({ content }: { content: string }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    await navigator.clipboard.writeText(content);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1200);
  };
  return (
    <button className="message-copy" type="button" onClick={() => void copy()} title="复制消息">
      {copied ? <Check size={14} /> : <Copy size={14} />}
    </button>
  );
}

function ConnectionBadge({ live, status }: { live: boolean; status: string }) {
  const connected = live && status === 'connected';
  return (
    <span className={connected ? 'chat-connection is-connected' : live ? 'chat-connection is-waiting' : 'chat-connection is-demo'}>
      {connected ? <Wifi size={14} /> : <WifiOff size={14} />}
      {connected ? '实时连接' : live ? '正在重连' : '演示模式'}
    </span>
  );
}

export function InWorldChatScreen(props: InWorldChatScreenProps) {
  const {
    messages,
    contacts,
    accountContacts,
    availableAgents,
    models,
    projects,
    runtimeSettings,
    activeProjectId,
    activeContactId,
    thinking,
    isStopping,
    loadingMessages,
    hasMoreMessages,
    sessionBusy,
    live,
    webSocketStatus,
    error,
    conversationId,
    conversationTitle,
    onSend,
    onStop,
    onSelectContact,
    onAddContact,
    onDeleteContact,
    onAssignProjectContact,
    onRemoveProjectContact,
    onRefresh,
    onLoadMore,
    onRuntimeChange,
    onProjectChange,
  } = props;
  const [input, setInput] = useState('');
  const [files, setFiles] = useState<PendingFile[]>([]);
  const [search, setSearch] = useState('');
  const [localError, setLocalError] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [pendingDeleteId, setPendingDeleteId] = useState<string | null>(null);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [pickerSelection, setPickerSelection] = useState<string | null>(null);
  const messageListRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const previewUrlsRef = useRef(new Set<string>());

  const activeModel = models.find((model) => model.id === runtimeSettings.selectedModelId) || models[0] || null;
  const activeProject = projects.find((project) => project.id === activeProjectId) || null;
  const filteredContacts = useMemo(() => {
    const query = search.trim().toLowerCase();
    return query ? contacts.filter((contact) => contact.name.toLowerCase().includes(query)) : contacts;
  }, [contacts, search]);
  const pickerOptions = activeProject
    ? accountContacts.filter((contact) => !contacts.some((item) => item.id === contact.id))
    : availableAgents;

  useEffect(() => {
    const list = messageListRef.current;
    if (!list) return;
    list.scrollTo({ top: list.scrollHeight, behavior: messages.length > 2 ? 'smooth' : 'auto' });
  }, [conversationId, messages, thinking]);

  useEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;
    textarea.style.height = 'auto';
    textarea.style.height = `${Math.min(180, Math.max(44, textarea.scrollHeight))}px`;
  }, [input]);

  useEffect(() => () => {
    previewUrlsRef.current.forEach((url) => URL.revokeObjectURL(url));
    previewUrlsRef.current.clear();
  }, []);

  const addFiles = (incoming: File[]) => {
    if (incoming.length === 0) return;
    try {
      const combined = [...files.map((item) => item.file), ...incoming];
      validateAttachmentFiles(combined);
      const additions = incoming.map((file, index) => {
        const previewUrl = file.type.startsWith('image/') ? URL.createObjectURL(file) : undefined;
        if (previewUrl) previewUrlsRef.current.add(previewUrl);
        return { id: `${file.name}-${file.lastModified}-${index}-${Date.now()}`, file, previewUrl };
      });
      setFiles((current) => [...current, ...additions]);
      setLocalError(null);
    } catch (cause) {
      setLocalError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  const removeFile = (id: string) => {
    setFiles((current) => current.filter((item) => {
      if (item.id !== id) return true;
      if (item.previewUrl) {
        URL.revokeObjectURL(item.previewUrl);
        previewUrlsRef.current.delete(item.previewUrl);
      }
      return false;
    }));
  };

  const clearFiles = () => {
    files.forEach((item) => {
      if (item.previewUrl) {
        URL.revokeObjectURL(item.previewUrl);
        previewUrlsRef.current.delete(item.previewUrl);
      }
    });
    setFiles([]);
  };

  const submitMessage = async () => {
    const content = input.trim();
    if ((!content && files.length === 0) || thinking || sending || !activeContactId) return;
    setSending(true);
    setLocalError(null);
    try {
      await onSend(content, files.map((item) => item.file));
      setInput('');
      clearFiles();
    } catch (cause) {
      setLocalError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setSending(false);
    }
  };

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault();
    void submitMessage();
  };

  return (
    <div className="inworld-chat-screen is-feature-complete">
      <aside className="chat-session-sidebar">
        <div className="screen-assistant">
          <Bot size={17} />
          <div><b>ChatOS</b><ConnectionBadge live={live} status={webSocketStatus} /></div>
        </div>

        <section className="chat-scope-switcher" aria-label="聊天模式切换">
          <div className="chat-scope-switcher__heading">
            <span>聊天模式</span>
            <em>选择沟通范围</em>
          </div>
          <div className="chat-scope-switcher__tabs">
            <button
              type="button"
              className={!activeProject ? 'is-active' : ''}
              aria-pressed={!activeProject}
              disabled={thinking || sessionBusy}
              onClick={() => void onProjectChange(null)}
            >
              <MessageSquareText size={17} />
              <span><b>联系人聊天</b><small>直接与 Agent 沟通</small></span>
            </button>
            <button
              type="button"
              className={activeProject ? 'is-active' : ''}
              aria-pressed={Boolean(activeProject)}
              disabled={thinking || sessionBusy || projects.length === 0}
              onClick={() => void onProjectChange(activeProjectId || projects[0]?.id || null)}
            >
              <FolderOpen size={17} />
              <span><b>项目聊天</b><small>{projects.length} 个用户项目</small></span>
            </button>
          </div>
          {activeProject ? (
            <label className="chat-scope-project-picker">
              <span>当前项目</span>
              <select
                aria-label="当前聊天项目"
                value={activeProject.id}
                onChange={(event) => void onProjectChange(event.target.value)}
                disabled={thinking || sessionBusy}
              >
                {projects.map((project) => <option value={project.id} key={project.id}>{project.name}</option>)}
              </select>
            </label>
          ) : (
            <div className="chat-scope-switcher__current"><i /><span>当前：联系人聊天</span></div>
          )}
        </section>

        <div className="chat-session-actions">
          <button type="button" onClick={() => { setPickerSelection(null); setPickerOpen(true); }} disabled={sessionBusy || thinking}>
            {activeProject ? <UsersRound size={15} /> : <UserPlus size={15} />}
            {activeProject ? '设置负责人' : '添加联系人'}
          </button>
          <button type="button" className="is-icon" onClick={() => void onRefresh()} disabled={sessionBusy} title="刷新联系人"><RefreshCw className={sessionBusy ? 'is-spinning' : ''} size={15} /></button>
        </div>
        <label className="chat-session-search"><Search size={14} /><input value={search} onChange={(event) => setSearch(event.target.value)} placeholder={activeProject ? '搜索项目负责人' : '搜索联系人'} /></label>

        <span className="screen-section-label">{activeProject ? `${activeProject.name} · 负责人` : '我的联系人'} · {contacts.length}</span>
        <div className="chat-session-list">
          {filteredContacts.map((contact) => (
            <div className={contact.id === activeContactId ? 'screen-session is-active' : 'screen-session'} key={contact.id}>
              <button className="screen-session__select" type="button" disabled={thinking} onClick={() => void onSelectContact(contact.id)}>
                <UserRound size={15} />
                <span><b>{contact.name}</b><small>{contact.sessionId ? contact.lastActive : activeProject ? '尚未开始项目沟通' : '新联系人'}</small></span>
              </button>
              {pendingDeleteId === contact.id ? (
                <span className="screen-session__confirm">
                  <button type="button" onClick={() => { setPendingDeleteId(null); void (activeProject ? onRemoveProjectContact(contact.id) : onDeleteContact(contact.id)); }}>{activeProject ? '移出' : '删除'}</button>
                  <button type="button" onClick={() => setPendingDeleteId(null)}>取消</button>
                </span>
              ) : (
                <button className="screen-session__delete" type="button" title={activeProject ? '移出项目负责人' : '删除联系人'} disabled={thinking || sessionBusy} onClick={() => setPendingDeleteId(contact.id)}><Trash2 size={14} /></button>
              )}
            </div>
          ))}
          {filteredContacts.length === 0 ? <div className="chat-session-empty">{search ? '没有匹配项' : activeProject ? '这个项目还没有负责人，请先从已有联系人中设置' : '还没有联系人，请先添加一个 Agent 联系人'}</div> : null}
        </div>

        <div className="screen-system-note">
          <span>{live ? 'ChatOS API + WebSocket' : '本地交互演示数据'}</span>
          <small>{messages.length} 条消息 · {activeProject ? `${contacts.length} 位项目负责人` : `${contacts.length} 位联系人`}</small>
        </div>
      </aside>

      <main className="chat-conversation-main">
        <header className="chat-conversation-header">
          <div className="chat-title-block"><b>{conversationTitle || (activeProject ? '请先设置项目负责人' : '请先添加联系人')}</b><span>{activeProject ? `${activeProject.name} · 与负责人沟通` : live ? '联系人聊天 · 真实记录' : '联系人聊天功能演示'}</span></div>
          <div className="chat-header-controls">
            <label title="当前模型"><Bot size={14} /><select aria-label="当前模型" value={runtimeSettings.selectedModelId || activeModel?.id || ''} onChange={(event) => {
              const model = models.find((item) => item.id === event.target.value);
              void onRuntimeChange({ selectedModelId: model?.id || null, selectedModelName: model?.modelName || null, selectedThinkingLevel: model?.thinkingLevel || null });
            }} disabled={thinking || models.length === 0}>
              {models.length === 0 ? <option value="">暂无模型</option> : models.map((model) => <option value={model.id} key={model.id}>{model.name}</option>)}
            </select></label>
            <label title="思考等级"><BrainCircuit size={14} /><select aria-label="思考等级" value={runtimeSettings.selectedThinkingLevel || 'auto'} onChange={(event) => void onRuntimeChange({ selectedThinkingLevel: event.target.value === 'auto' ? null : event.target.value })} disabled={thinking}>
              <option value="auto">自动</option><option value="low">低</option><option value="medium">中</option><option value="high">高</option><option value="xhigh">极高</option>
            </select></label>
          </div>
        </header>

        <div className="screen-message-list" ref={messageListRef}>
          {hasMoreMessages ? <button className="load-more-messages" type="button" disabled={loadingMessages} onClick={() => void onLoadMore()}>{loadingMessages ? <LoaderCircle className="is-spinning" size={15} /> : null}{loadingMessages ? '正在加载…' : '加载更早的消息'}</button> : null}
          {loadingMessages && messages.length === 0 ? <div className="chat-empty-state"><LoaderCircle className="is-spinning" size={24} /><b>正在读取聊天记录</b></div> : null}
          {!loadingMessages && messages.length === 0 ? <div className="chat-empty-state">{activeContactId ? <MessageSquareText size={30} /> : <UsersRound size={30} />}<b>{activeContactId ? `开始与${conversationTitle || '负责人'}沟通` : activeProject ? '项目尚未设置负责人' : '先添加一个 Agent 联系人'}</b><span>{activeContactId ? '首次发送消息时才会建立对应会话。' : activeProject ? '负责人来自你的联系人列表，可在上方进行设置。' : '和添加好友一样，选择一个可用 Agent 加入联系人。'}</span></div> : null}
          {messages.map((message) => (
            <article className={`${message.role === 'user' ? 'is-user' : 'is-assistant'}${message.status === 'error' ? ' is-error' : ''}`} key={message.id}>
              <div>{message.role === 'user' ? '你' : <Bot size={17} />}</div>
              <section className="message-content-shell">
                {message.content ? <MarkdownMessage content={message.content} /> : null}
                {message.attachments?.length ? <div className="message-attachments">{message.attachments.map((attachment) => <span key={attachment.id || attachment.name}>{attachment.type === 'image' ? <ImageIcon size={14} /> : <FileText size={14} />}<b>{attachment.name}</b><small>{formatFileSize(attachment.size)}</small></span>)}</div> : null}
                <MessageCopyButton content={message.content} />
              </section>
              <time>{message.status === 'sending' ? '发送中' : message.status === 'error' ? '发送失败' : message.time}</time>
            </article>
          ))}
          {thinking ? <div className="screen-thinking"><Bot size={15} /><span>{isStopping ? '正在停止…' : 'AI 正在思考'}</span><i /><i /><i /></div> : null}
        </div>

        <form className="chat-composer-full" onSubmit={handleSubmit} onDragOver={(event) => { event.preventDefault(); event.currentTarget.classList.add('is-dragging'); }} onDragLeave={(event) => event.currentTarget.classList.remove('is-dragging')} onDrop={(event) => { event.preventDefault(); event.currentTarget.classList.remove('is-dragging'); addFiles(Array.from(event.dataTransfer.files)); }}>
          {(localError || error) ? <div className="chat-inline-error"><X size={14} />{localError || error}</div> : null}
          {files.length > 0 ? <div className="pending-attachments">{files.map((item) => <span key={item.id}>{item.previewUrl ? <img src={item.previewUrl} alt="" /> : <FileText size={16} />}<b>{item.file.name}</b><small>{formatFileSize(item.file.size)}</small><button type="button" aria-label={`移除附件 ${item.file.name}`} onClick={() => removeFile(item.id)}><X size={13} /></button></span>)}</div> : null}
          <div className="chat-composer-row">
            <button type="button" className="composer-attach" onClick={() => fileInputRef.current?.click()} disabled={thinking || sending} title="添加附件"><Paperclip size={19} /></button>
            <input ref={fileInputRef} hidden type="file" multiple onChange={(event) => { addFiles(Array.from(event.target.files || [])); event.target.value = ''; }} />
            <textarea
              ref={textareaRef}
              value={input}
              maxLength={MAX_MESSAGE_LENGTH}
              onChange={(event) => setInput(event.target.value)}
              onPaste={(event) => {
                const pastedFiles = Array.from(event.clipboardData.files);
                if (pastedFiles.length > 0) addFiles(pastedFiles);
              }}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) {
                  event.preventDefault();
                  void submitMessage();
                }
              }}
              placeholder={thinking ? 'AI 正在回复，可点击停止…' : '输入消息，Enter 发送，Shift + Enter 换行'}
              disabled={!activeContactId || sending}
            />
            <span className="composer-count">{input.length}/{MAX_MESSAGE_LENGTH}</span>
            {thinking ? <button className="composer-stop" type="button" disabled={isStopping} onClick={() => void onStop()}><Square size={17} />{isStopping ? '停止中' : '停止'}</button> : <button className="composer-send" type="submit" disabled={(!input.trim() && files.length === 0) || sending || !activeContactId}><Send size={18} />发送</button>}
          </div>
          <footer>
            <button type="button" className={runtimeSettings.reasoningEnabled ? 'is-active' : ''} disabled={thinking || activeModel?.supportsReasoning === false} onClick={() => void onRuntimeChange({ reasoningEnabled: !runtimeSettings.reasoningEnabled })}><BrainCircuit size={14} />Reasoning {runtimeSettings.reasoningEnabled ? '开' : '关'}</button>
            <button type="button" className={runtimeSettings.planModeEnabled ? 'is-active' : ''} disabled={thinking || !activeProjectId} onClick={() => void onRuntimeChange({ planModeEnabled: !runtimeSettings.planModeEnabled })}><ListChecks size={14} />Plan Mode {runtimeSettings.planModeEnabled ? '开' : '关'}</button>
            <span>支持拖拽或粘贴附件 · 总计不超过 20 MB</span>
          </footer>
        </form>
      </main>
      {pickerOpen ? (
        <div className="contact-picker-backdrop" role="dialog" aria-modal="true" aria-label={activeProject ? '设置项目负责人' : '添加联系人'}>
          <section className="contact-picker-dialog">
            <header>
              <div>{activeProject ? <UsersRound size={20} /> : <UserPlus size={20} />}<span><b>{activeProject ? '设置项目负责人' : '添加 Agent 联系人'}</b><small>{activeProject ? `从已有联系人中选择，加入 ${activeProject.name}` : '像添加好友一样，先选择一个可用 Agent'}</small></span></div>
              <button type="button" onClick={() => setPickerOpen(false)} aria-label="关闭"><X size={17} /></button>
            </header>
            <div className="contact-picker-list">
              {pickerOptions.map((option) => (
                <button type="button" className={pickerSelection === option.id ? 'is-selected' : ''} key={option.id} onClick={() => setPickerSelection(option.id)}>
                  <i><Bot size={18} /></i>
                  <span><b>{option.name}</b><small>{option.description || ('agentId' in option ? option.agentId : option.id)}</small></span>
                  <em>{pickerSelection === option.id ? <Check size={15} /> : null}</em>
                </button>
              ))}
              {pickerOptions.length === 0 ? <div className="contact-picker-empty">{activeProject ? '所有联系人都已加入这个项目；如需新的负责人，请先到联系人聊天中添加 Agent。' : '没有更多可添加的 Agent。'}</div> : null}
            </div>
            <footer>
              <button type="button" onClick={() => setPickerOpen(false)}>取消</button>
              <button type="button" disabled={!pickerSelection || sessionBusy} onClick={async () => {
                if (!pickerSelection) return;
                if (activeProject) await onAssignProjectContact(pickerSelection);
                else await onAddContact(pickerSelection);
                setPickerOpen(false);
                setPickerSelection(null);
              }}>{sessionBusy ? <LoaderCircle className="is-spinning" size={14} /> : null}{activeProject ? '设为负责人' : '添加联系人'}</button>
            </footer>
          </section>
        </div>
      ) : null}
    </div>
  );
}
