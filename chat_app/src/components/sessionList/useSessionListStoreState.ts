import { shallow } from 'zustand/shallow';

import { useChatStore } from '../../lib/store';

type SessionListStoreHook = typeof useChatStore;

export const useSessionListStoreState = (storeToUse: SessionListStoreHook) => {
  return storeToUse((state) => ({
    sessions: state.sessions,
    contacts: state.contacts,
    agents: state.agents,
    currentSession: state.currentSession,
    activePanel: state.activePanel,
    loadContacts: state.loadContacts,
    createContact: state.createContact,
    deleteContact: state.deleteContact,
    createSession: state.createSession,
    selectSession: state.selectSession,
    deleteSession: state.deleteSession,
    updateSession: state.updateSession,
    loadAgents: state.loadAgents,
    sessionChatState: state.sessionChatState,
    taskReviewPanelsBySession: state.taskReviewPanelsBySession,
    uiPromptPanelsBySession: state.uiPromptPanelsBySession,
    projects: state.projects,
    currentProject: state.currentProject,
    loadProjects: state.loadProjects,
    createProject: state.createProject,
    selectProject: state.selectProject,
    deleteProject: state.deleteProject,
    setActivePanel: state.setActivePanel,
    terminals: state.terminals,
    currentTerminal: state.currentTerminal,
    loadTerminals: state.loadTerminals,
    createTerminal: state.createTerminal,
    selectTerminal: state.selectTerminal,
    deleteTerminal: state.deleteTerminal,
    remoteConnections: state.remoteConnections,
    currentRemoteConnection: state.currentRemoteConnection,
    loadRemoteConnections: state.loadRemoteConnections,
    createRemoteConnection: state.createRemoteConnection,
    updateRemoteConnection: state.updateRemoteConnection,
    selectRemoteConnection: state.selectRemoteConnection,
    deleteRemoteConnection: state.deleteRemoteConnection,
    openRemoteSftp: state.openRemoteSftp,
  }), shallow);
};
