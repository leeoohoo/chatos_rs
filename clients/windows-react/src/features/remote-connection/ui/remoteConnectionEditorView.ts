import type { useRemoteConnectionEditorStore } from "../application/remoteConnectionEditorStore";
import type { RemoteConnectionDraft } from "../domain/remoteConnectionDraft";
import type { RemoteCredentialFileKind } from "../domain/remoteConnection";

export interface RemoteConnectionEditorStateView {
  state: ReturnType<typeof useRemoteConnectionEditorStore.getState>;
  update(patch: Partial<RemoteConnectionDraft>): void;
  pick(kind: RemoteCredentialFileKind): void;
}
