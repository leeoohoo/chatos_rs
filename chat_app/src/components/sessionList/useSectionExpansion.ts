import { useCallback, useState } from 'react';

interface UseSectionExpansionOptions {
  onFocusTerminal: () => void;
  onFocusRemote: () => void;
}

interface UseSectionExpansionResult {
  sessionsExpanded: boolean;
  projectsExpanded: boolean;
  terminalsExpanded: boolean;
  remoteExpanded: boolean;
  handleToggleSessionsSection: () => void;
  handleToggleProjectsSection: () => void;
  handleToggleTerminalsSection: () => void;
  handleToggleRemoteSection: () => void;
}

export const useSectionExpansion = ({
  onFocusTerminal,
  onFocusRemote,
}: UseSectionExpansionOptions): UseSectionExpansionResult => {
  const [sessionsExpanded, setSessionsExpanded] = useState(true);
  const [projectsExpanded, setProjectsExpanded] = useState(true);
  const [terminalsExpanded, setTerminalsExpanded] = useState(true);
  const [remoteExpanded, setRemoteExpanded] = useState(true);

  const handleToggleSessionsSection = useCallback(() => {
    setSessionsExpanded((prev) => {
      const next = !prev;
      if (next) {
        setProjectsExpanded(false);
        setTerminalsExpanded(false);
        setRemoteExpanded(false);
      }
      return next;
    });
  }, []);

  const handleToggleProjectsSection = useCallback(() => {
    setProjectsExpanded((prev) => {
      const next = !prev;
      if (next) {
        setSessionsExpanded(false);
        setTerminalsExpanded(false);
        setRemoteExpanded(false);
      }
      return next;
    });
  }, []);

  const handleToggleTerminalsSection = useCallback(() => {
    setTerminalsExpanded((prev) => {
      const next = !prev;
      if (next) {
        setSessionsExpanded(false);
        setProjectsExpanded(false);
        setRemoteExpanded(false);
        onFocusTerminal();
      }
      return next;
    });
  }, [onFocusTerminal]);

  const handleToggleRemoteSection = useCallback(() => {
    setRemoteExpanded((prev) => {
      const next = !prev;
      if (next) {
        setSessionsExpanded(false);
        setProjectsExpanded(false);
        setTerminalsExpanded(false);
        onFocusRemote();
      }
      return next;
    });
  }, [onFocusRemote]);

  return {
    sessionsExpanded,
    projectsExpanded,
    terminalsExpanded,
    remoteExpanded,
    handleToggleSessionsSection,
    handleToggleProjectsSection,
    handleToggleTerminalsSection,
    handleToggleRemoteSection,
  };
};
