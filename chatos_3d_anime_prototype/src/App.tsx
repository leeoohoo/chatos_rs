import { Canvas } from '@react-three/fiber';
import { Cat as CatIcon, X } from 'lucide-react';
import { Suspense, useEffect, useRef, useState } from 'react';
import { ACESFilmicToneMapping, SRGBColorSpace } from 'three';
import {
  ComputerFocusLayer,
  FocusDesktop,
  InWorldLoginScreen,
  InWorldRemoteScreen,
  InWorldTerminalScreen,
  PhoneWorkspace,
} from './app/computerWorkspaces';
import {
  DEMO_AVAILABLE_AGENTS,
  DEMO_CHAT_CONTACTS,
  DEMO_CHAT_MODELS,
  DEMO_CHAT_SESSIONS,
  DEMO_PROJECT_CONTACT_IDS,
  DEMO_RUNTIME_SETTINGS,
  DEMO_SESSION_CONTACT_IDS,
  DEMO_SESSION_MESSAGES,
  DEMO_TASK_GRAPH,
  EMPTY_DEMO_TASK_GRAPH,
} from './app/demoState';
import { BottomNavigation, RoomHint, SceneLoading, TopBar } from './app/navigation';
import { ProjectDossierFocusLayer, SpatialModeHint } from './app/projectDossier';
import { InWorldTaskWall } from './app/taskCenter';
import { demoProjects, demoTasks } from './demoData';
import { InWorldChatScreen } from './InWorldChatScreen';
import { RealisticRoom } from './scene/AnimeRoom';
import type {
  ChatAgentOption,
  ChatContact,
  ChatMessage,
  ChatRuntimeSettings,
  ChatSession,
  DemoProject,
  DemoTask,
  TimeMode,
  ViewMode,
} from './types';
import { useChatOSBridge } from './useChatOSBridge';

const formatTime = () => new Intl.DateTimeFormat('zh-CN', {
  hour: '2-digit',
  minute: '2-digit',
  hour12: false,
}).format(new Date());

function App() {
  const bridge = useChatOSBridge();
  const [view, setView] = useState<ViewMode>('room');
  const [timeMode, setTimeMode] = useState<TimeMode>('day');
  const [selectedProject, setSelectedProject] = useState<DemoProject>(demoProjects[0]);
  const [selectedTask, setSelectedTask] = useState<DemoTask | null>(demoTasks.find((task) => task.status === 'doing') || null);
  const [demoSessions, setDemoSessions] = useState<ChatSession[]>(DEMO_CHAT_SESSIONS);
  const [demoSessionContactIds, setDemoSessionContactIds] = useState<Record<string, string>>(DEMO_SESSION_CONTACT_IDS);
  const [demoContacts, setDemoContacts] = useState<ChatContact[]>(DEMO_CHAT_CONTACTS);
  const [demoAvailableAgents, setDemoAvailableAgents] = useState<ChatAgentOption[]>(DEMO_AVAILABLE_AGENTS);
  const [demoProjectContactIds, setDemoProjectContactIds] = useState<Record<string, string[]>>(DEMO_PROJECT_CONTACT_IDS);
  const [demoActiveContactId, setDemoActiveContactId] = useState<string | null>('contact-architect');
  const [demoConversationId, setDemoConversationId] = useState<string | null>('demo-room');
  const [demoMessagesBySession, setDemoMessagesBySession] = useState<Record<string, ChatMessage[]>>(DEMO_SESSION_MESSAGES);
  const [demoRuntimeSettings, setDemoRuntimeSettings] = useState<ChatRuntimeSettings>(DEMO_RUNTIME_SETTINGS);
  const [demoActiveProjectId, setDemoActiveProjectId] = useState<string | null>(DEMO_CHAT_SESSIONS[0].projectId);
  const [demoThinking, setDemoThinking] = useState(false);
  const [demoDesktop, setDemoDesktop] = useState(false);
  const [catToast, setCatToast] = useState(false);
  const demoThinkingTimerRef = useRef<number | null>(null);
  const live = bridge.status === 'live';
  const projects = live ? bridge.projects : demoProjects;
  const tasks = live ? bridge.runningTasks : demoTasks;
  const runningTaskPreview = tasks.filter((task) => task.status === 'doing');
  const messages = live ? bridge.messages : (demoConversationId ? demoMessagesBySession[demoConversationId] || [] : []);
  const demoScopedContactIds = demoActiveProjectId ? (demoProjectContactIds[demoActiveProjectId] || []) : demoContacts.map((contact) => contact.id);
  const demoScopedContacts = demoContacts.filter((contact) => demoScopedContactIds.includes(contact.id)).map((contact) => {
    const session = demoSessions.find((item) => item.projectId === demoActiveProjectId && demoSessionContactIds[item.id] === contact.id);
    return { ...contact, projectId: demoActiveProjectId, sessionId: session?.id || null, lastActive: session?.updatedAt || contact.lastActive };
  });
  const chatContacts = live ? bridge.contacts : demoScopedContacts;
  const accountContacts = live ? bridge.accountContacts : demoContacts;
  const availableAgents = live ? bridge.availableAgents : demoAvailableAgents;
  const models = live ? bridge.models : DEMO_CHAT_MODELS;
  const runtimeSettings = live ? bridge.runtimeSettings : demoRuntimeSettings;
  const activeProjectId = live ? bridge.activeProjectId : demoActiveProjectId;
  const thinking = live ? bridge.thinking : demoThinking;
  const activeContactId = live ? bridge.activeContactId : demoActiveContactId;
  const conversationId = live ? bridge.conversationId : demoConversationId;
  const conversationTitle = live
    ? bridge.conversationTitle
    : demoContacts.find((contact) => contact.id === demoActiveContactId)?.name || null;
  const computerUnlocked = live || demoDesktop;

  useEffect(() => {
    if (projects.length === 0) return;
    setSelectedProject((current) => projects.find((project) => project.id === current.id) || projects[0]);
  }, [projects]);

  useEffect(() => {
    setSelectedTask((current) => current ? tasks.find((task) => task.id === current.id) || tasks[0] || null : tasks[0] || null);
  }, [tasks]);

  useEffect(() => () => {
    if (demoThinkingTimerRef.current !== null) window.clearTimeout(demoThinkingTimerRef.current);
  }, []);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setView((current) => {
          if (current === 'project') return 'archive';
          if (current === 'chat' || current === 'terminal' || current === 'remote') return 'computer';
          return 'room';
        });
        return;
      }
      if (view !== 'room') return;
      if (event.key === 'ArrowLeft') setView('archive');
      if (event.key === 'ArrowRight') setView('projection');
      if (event.key === 'Enter') setView('computer');
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [view]);

  const handleProjectSelect = (project: DemoProject) => {
    setSelectedProject(project);
    setView('project');
    if (bridge.status === 'live') void bridge.selectProject(project.id);
  };

  const handleSend = async (content: string, files: File[] = []) => {
    if (live) {
      await bridge.sendMessage(content, files);
      return;
    }
    if (!demoActiveContactId) throw new Error('请先选择联系人或项目负责人');
    let targetConversationId = demoConversationId;
    if (!targetConversationId) {
      targetConversationId = `demo-${Date.now()}`;
      const contact = demoContacts.find((item) => item.id === demoActiveContactId);
      const session: ChatSession = { id: targetConversationId, title: contact?.name || '联系人', projectId: demoActiveProjectId, updatedAt: '刚刚', archived: false };
      setDemoSessions((current) => [session, ...current]);
      setDemoSessionContactIds((current) => ({ ...current, [targetConversationId as string]: demoActiveContactId }));
      setDemoMessagesBySession((current) => ({ ...current, [targetConversationId as string]: [] }));
      setDemoConversationId(targetConversationId);
    }
    const activeConversationId = targetConversationId;
    const userMessage: ChatMessage = {
      id: `user-${Date.now()}`,
      role: 'user',
      content,
      time: formatTime(),
      attachments: files.map((file, index) => ({
        id: `demo-file-${Date.now()}-${index}`,
        name: file.name,
        mimeType: file.type || 'application/octet-stream',
        size: file.size,
        type: file.type.startsWith('image/') ? 'image' : file.type.startsWith('audio/') ? 'audio' : 'file',
      })),
    };
    setDemoMessagesBySession((current) => ({
      ...current,
      [activeConversationId]: [...(current[activeConversationId] || []), userMessage],
    }));
    setDemoSessions((current) => current.map((session) => session.id === activeConversationId ? { ...session, updatedAt: '刚刚' } : session));
    setDemoThinking(true);

    demoThinkingTimerRef.current = window.setTimeout(() => {
      const assistantMessage: ChatMessage = {
        id: `assistant-${Date.now()}`,
        role: 'assistant',
        content: files.length > 0
          ? `收到消息和 ${files.length} 个附件。演示模式已经完成附件预览与发送流程；登录真实 ChatOS 后会按原前端格式传给后端。`
          : '收到。联系人和项目负责人关系已经按原前端处理；首次发送消息时才会自动建立对应会话。',
        time: formatTime(),
      };
      setDemoMessagesBySession((current) => ({
        ...current,
        [activeConversationId]: [...(current[activeConversationId] || []), assistantMessage],
      }));
      setDemoThinking(false);
      demoThinkingTimerRef.current = null;
    }, 850);
  };

  const stopDemoMessage = () => {
    if (demoThinkingTimerRef.current !== null) window.clearTimeout(demoThinkingTimerRef.current);
    demoThinkingTimerRef.current = null;
    setDemoThinking(false);
  };

  const selectDemoContact = (contactId: string) => {
    setDemoActiveContactId(contactId);
    const session = demoSessions.find((item) => item.projectId === demoActiveProjectId && demoSessionContactIds[item.id] === contactId);
    setDemoConversationId(session?.id || null);
  };

  const addDemoContact = (agentId: string) => {
    const agent = demoAvailableAgents.find((item) => item.id === agentId);
    if (!agent) return;
    const contact: ChatContact = { id: `contact-${Date.now()}`, agentId: agent.id, name: agent.name, description: agent.description, sessionId: null, projectId: null, lastActive: '刚刚' };
    setDemoContacts((current) => [...current, contact]);
    setDemoAvailableAgents((current) => current.filter((item) => item.id !== agentId));
    setDemoActiveProjectId(null);
    setDemoActiveContactId(contact.id);
    setDemoConversationId(null);
  };

  const deleteDemoContact = (contactId: string) => {
    const remaining = demoContacts.filter((contact) => contact.id !== contactId);
    setDemoContacts(remaining);
    setDemoProjectContactIds((current) => Object.fromEntries(Object.entries(current).map(([projectId, ids]) => [projectId, ids.filter((id) => id !== contactId)])));
    if (demoActiveContactId === contactId) {
      setDemoActiveContactId(remaining[0]?.id || null);
      const session = remaining[0] ? demoSessions.find((item) => item.projectId === null && demoSessionContactIds[item.id] === remaining[0].id) : null;
      setDemoConversationId(session?.id || null);
    }
  };

  const assignDemoProjectContact = (contactId: string) => {
    if (!demoActiveProjectId) return;
    setDemoProjectContactIds((current) => ({ ...current, [demoActiveProjectId]: Array.from(new Set([...(current[demoActiveProjectId] || []), contactId])) }));
    setDemoActiveContactId(contactId);
    const session = demoSessions.find((item) => item.projectId === demoActiveProjectId && demoSessionContactIds[item.id] === contactId);
    setDemoConversationId(session?.id || null);
  };

  const removeDemoProjectContact = (contactId: string) => {
    if (!demoActiveProjectId) return;
    const remaining = (demoProjectContactIds[demoActiveProjectId] || []).filter((id) => id !== contactId);
    setDemoProjectContactIds((current) => ({ ...current, [demoActiveProjectId]: remaining }));
    if (demoActiveContactId === contactId) {
      const nextId = remaining[0] || null;
      setDemoActiveContactId(nextId);
      const session = nextId ? demoSessions.find((item) => item.projectId === demoActiveProjectId && demoSessionContactIds[item.id] === nextId) : null;
      setDemoConversationId(session?.id || null);
    }
  };

  const handleChatProjectChange = async (projectId: string | null) => {
    if (live) {
      if (projectId) await bridge.selectProject(projectId);
      else await bridge.selectPersonalContacts();
      return;
    }
    setDemoActiveProjectId(projectId);
    const contactId = projectId ? (demoProjectContactIds[projectId] || [])[0] || null : demoContacts[0]?.id || null;
    setDemoActiveContactId(contactId);
    const session = contactId ? demoSessions.find((item) => item.projectId === projectId && demoSessionContactIds[item.id] === contactId) : null;
    setDemoConversationId(session?.id || null);
  };

  const handleRuntimeChange = async (patch: Partial<ChatRuntimeSettings>) => {
    if (live) {
      await bridge.updateRuntimeSettings(patch);
      return;
    }
    setDemoRuntimeSettings((current) => ({ ...current, ...patch }));
  };

  const handleCatPet = () => {
    setCatToast(true);
    window.setTimeout(() => setCatToast(false), 2200);
  };

  useEffect(() => {
    if (tasks.length === 0) {
      setSelectedTask(null);
      return;
    }
    if (!selectedTask || !tasks.some((task) => task.id === selectedTask.id)) {
      setSelectedTask(tasks.find((task) => task.status === 'doing') || tasks[0]);
    }
  }, [selectedTask, tasks]);

  useEffect(() => {
    if (!live || view !== 'projection' || !selectedTask) return;
    void bridge.loadTaskGraph(selectedTask);
  }, [live, selectedTask, view]); // eslint-disable-line react-hooks/exhaustive-deps

  const handleTaskCenterRefresh = () => {
    if (!live) return;
    void bridge.refresh();
    if (selectedTask) void bridge.loadTaskGraph(selectedTask);
  };

  const computerImmersive = view === 'computer' || view === 'chat' || view === 'terminal' || view === 'remote';
  const projectImmersive = view === 'project';
  const taskImmersive = view === 'projection';
  const immersiveView = computerImmersive || projectImmersive || taskImmersive;
  const activeSelectedProject = projects.find((project) => project.id === selectedProject.id) || selectedProject;
  const activeTaskGraph = live
    ? bridge.taskGraphTaskId === selectedTask?.id ? bridge.taskGraph : EMPTY_DEMO_TASK_GRAPH
    : DEMO_TASK_GRAPH;
  const activeTaskGraphLoading = live && Boolean(selectedTask)
    ? bridge.taskGraphTaskId !== selectedTask?.id || bridge.taskGraphLoading
    : false;
  const activeTaskGraphError = live && bridge.taskGraphTaskId === selectedTask?.id ? bridge.taskGraphError : null;
  const exitImmersiveView = () => {
    setView((current) => current === 'chat' || current === 'terminal' || current === 'remote' ? 'computer' : 'room');
  };

  const lockComputer = () => {
    if (bridge.user) bridge.logout();
    setDemoDesktop(false);
    setView('computer');
  };

  return (
    <div className={`app-shell time-${timeMode}${projectImmersive ? ' is-project-reading' : ''}${taskImmersive ? ' is-task-center' : ''}`}>
      <div className={projectImmersive ? 'room-stage is-project-hidden' : taskImmersive ? 'room-stage is-task-hidden' : computerImmersive ? 'room-stage is-suspended' : 'room-stage'} aria-hidden={computerImmersive || projectImmersive || taskImmersive}>
        <Suspense fallback={<SceneLoading />}>
          <Canvas
          className="room-canvas"
          frameloop="demand"
          shadows="percentage"
          dpr={[1, 1.5]}
          camera={{ position: [0, 2.62, 6.7], fov: 47, near: 0.1, far: 60 }}
          gl={{ antialias: true, alpha: false, powerPreference: 'high-performance' }}
          onCreated={({ gl }) => {
            gl.toneMapping = ACESFilmicToneMapping;
            gl.toneMappingExposure = timeMode === 'night' ? 0.74 : timeMode === 'sunset' ? 0.79 : 0.82;
            gl.outputColorSpace = SRGBColorSpace;

            const canvas = gl.domElement;
            const handleContextLost = (event: Event) => {
              event.preventDefault();
            };
            const handleContextRestored = () => {
              gl.resetState();
            };
            canvas.addEventListener('webglcontextlost', handleContextLost, false);
            canvas.addEventListener('webglcontextrestored', handleContextRestored, false);
          }}
          onPointerMissed={() => {
            if (view === 'room') return;
          }}
          >
            <RealisticRoom
            view={view}
            timeMode={timeMode}
            projects={projects}
            tasks={runningTaskPreview}
            computerLocked={!computerUnlocked}
            computerScreenCovered={computerImmersive}
            computerContent={null}
            taskWallContent={null}
            onViewChange={setView}
            onComputerLock={lockComputer}
            onProjectSelect={handleProjectSelect}
            onCatPet={handleCatPet}
            />
          </Canvas>
        </Suspense>
      </div>

      <div className="vignette" />
      {computerImmersive ? (
        <ComputerFocusLayer>
          {view === 'computer' ? (
            computerUnlocked ? (
              <FocusDesktop
                timeMode={timeMode}
                onChat={() => setView('chat')}
                onTerminal={() => setView('terminal')}
                onRemote={() => setView('remote')}
                onLock={lockComputer}
              />
            ) : (
              <InWorldLoginScreen bridge={bridge} onDemo={() => setDemoDesktop(true)} />
            )
          ) : view === 'terminal' ? (
            <InWorldTerminalScreen />
          ) : view === 'remote' ? (
            <InWorldRemoteScreen />
          ) : (
            <InWorldChatScreen
              messages={messages}
              contacts={chatContacts}
              accountContacts={accountContacts}
              availableAgents={availableAgents}
              models={models}
              projects={projects}
              runtimeSettings={runtimeSettings}
              activeProjectId={activeProjectId}
              activeContactId={activeContactId}
              thinking={thinking}
              isStopping={live ? bridge.isStopping : false}
              loadingMessages={live ? bridge.loadingMessages : false}
              hasMoreMessages={live ? bridge.hasMoreMessages : false}
              sessionBusy={live ? bridge.sessionBusy : false}
              onSend={handleSend}
              onStop={live ? bridge.stopMessage : stopDemoMessage}
              live={live}
              webSocketStatus={live ? bridge.webSocketStatus : 'demo'}
              error={live ? bridge.error : null}
              conversationId={conversationId}
              conversationTitle={conversationTitle}
              onSelectContact={live ? bridge.selectContact : selectDemoContact}
              onAddContact={live ? bridge.addContact : addDemoContact}
              onDeleteContact={live ? bridge.deleteContact : deleteDemoContact}
              onAssignProjectContact={live ? bridge.assignProjectContact : assignDemoProjectContact}
              onRemoveProjectContact={live ? bridge.removeProjectContact : removeDemoProjectContact}
              onRefresh={live ? bridge.refresh : () => undefined}
              onLoadMore={live ? bridge.loadMoreMessages : () => undefined}
              onRuntimeChange={handleRuntimeChange}
              onProjectChange={handleChatProjectChange}
            />
          )}
        </ComputerFocusLayer>
      ) : null}
      {projectImmersive ? (
        <ProjectDossierFocusLayer
          project={activeSelectedProject}
          onClose={() => setView('archive')}
        />
      ) : null}
      {taskImmersive ? (
        <InWorldTaskWall
          tasks={tasks}
          selectedTask={selectedTask}
          onSelect={setSelectedTask}
          timeMode={timeMode}
          graph={activeTaskGraph}
          graphLoading={activeTaskGraphLoading}
          graphError={activeTaskGraphError}
          onRefresh={handleTaskCenterRefresh}
          onClose={() => setView('room')}
        />
      ) : null}
      {!immersiveView ? <TopBar view={view} timeMode={timeMode} onTimeModeChange={setTimeMode} /> : null}

      {view === 'room' ? <RoomHint /> : null}
      {!immersiveView ? <SpatialModeHint view={view} projectName={activeSelectedProject.name} /> : null}
      {computerImmersive ? (
        <button className={computerImmersive ? 'immersive-exit is-computer' : 'immersive-exit'} type="button" onClick={exitImmersiveView}>
          <X size={15} />
          <span>{view === 'chat' || view === 'terminal' || view === 'remote' ? '返回桌面' : '退出全屏'}</span>
        </button>
      ) : null}
      {view === 'phone' ? (
        <PhoneWorkspace
          timeMode={timeMode}
          onTimeModeChange={setTimeMode}
          onClose={() => setView('room')}
        />
      ) : null}

      {catToast ? (
        <div className="cat-toast">
          <CatIcon size={20} />
          <span>小猫发出了满意的呼噜声。</span>
        </div>
      ) : null}

      {!immersiveView ? <BottomNavigation view={view} onViewChange={setView} /> : null}

    </div>
  );
}

export default App;
