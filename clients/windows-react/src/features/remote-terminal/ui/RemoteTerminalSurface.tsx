import { forwardRef } from "react";
import { TerminalSurface, type TerminalSurfaceHandle } from "../../terminal/ui/TerminalSurface";
import { useRemoteTerminalStore } from "../application/remoteTerminalStore";
import { workingDirectoryFromOsc } from "../domain/remoteTerminal";
import type { RemoteTerminalPort } from "../ports/RemoteTerminalPort";

export type RemoteTerminalSurfaceHandle = TerminalSurfaceHandle;

export const RemoteTerminalSurface = forwardRef<
  RemoteTerminalSurfaceHandle,
  { port: RemoteTerminalPort }
>(function RemoteTerminalSurface({ port }, ref) {
  const events = useRemoteTerminalStore((state) => state.events);
  const outputGeneration = useRemoteTerminalStore((state) => state.outputGeneration);
  return (
    <TerminalSurface
      ariaLabel="远程终端输出"
      className="remote-terminal-surface"
      events={events}
      lifecycleKey={port}
      onInput={(data) => useRemoteTerminalStore.getState().sendInput(port, data)}
      onOsc7={(value) => {
        const directory = workingDirectoryFromOsc(value);
        if (directory) useRemoteTerminalStore.getState().updateWorkingDirectory(directory);
      }}
      onResize={(cols, rows) => { void useRemoteTerminalStore.getState().resize(port, cols, rows); }}
      onTitleChange={(title) => useRemoteTerminalStore.getState().updateTitle(title)}
      outputGeneration={outputGeneration}
      ref={ref}
    />
  );
});
