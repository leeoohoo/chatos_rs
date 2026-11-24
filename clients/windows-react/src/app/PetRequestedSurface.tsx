import { lazy, Suspense, type ComponentProps } from "react";
import type { PetSurfaceRequest } from "../features/pet/ports/PetSurfaceRequestPort";
import type { WorkspaceIndexSnapshot } from "../features/workspace-index/domain/workspaceIndex";

const NotepadWorkspace = lazy(async () => {
  const [adapterModule, viewModule] = await Promise.all([
    import("../features/notepad/adapters/TauriNotepadAdapter"),
    import("../features/notepad/ui/NotepadView"),
  ]);
  const port = new adapterModule.TauriNotepadAdapter();
  type Props = Omit<ComponentProps<typeof viewModule.NotepadView>, "port">;
  return {
    default: (props: Props) => <viewModule.NotepadView {...props} port={port} />,
  };
});

const PetQuickChatWorkspace = lazy(async () => {
  const module = await import("../features/pet/ui/PetQuickChatSurface");
  return { default: module.PetQuickChatSurface };
});

interface PetRequestedSurfaceProps {
  hostReady: boolean;
  loadingLabel: string;
  ownerUserId: string;
  resources: WorkspaceIndexSnapshot;
  surface: PetSurfaceRequest | null;
  onChange(surface: PetSurfaceRequest | null): void;
  onOpenTranslation(): void;
  onSelectResource(kind: "contact" | "project", id: string): void;
}

export function PetRequestedSurface({
  hostReady,
  loadingLabel,
  ownerUserId,
  resources,
  surface,
  onChange,
  onOpenTranslation,
  onSelectResource,
}: PetRequestedSurfaceProps) {
  if (surface === "notepad") {
    return (
      <Suspense fallback={<div className="notepad-sheet-backdrop"><div className="workspace-loading"><span className="mini-spinner" />{loadingLabel}</div></div>}>
        <NotepadWorkspace
          hostReady={hostReady}
          ownerUserId={ownerUserId}
          onClose={() => onChange(null)}
        />
      </Suspense>
    );
  }
  if (surface !== "quick-chat") return null;
  return (
    <Suspense fallback={null}>
      <PetQuickChatWorkspace
        onClose={() => onChange(null)}
        onOpenNotepad={() => onChange("notepad")}
        onOpenTranslation={onOpenTranslation}
        onSelectResource={(kind, id) => {
          onChange(null);
          onSelectResource(kind, id);
        }}
        resources={resources}
      />
    </Suspense>
  );
}
