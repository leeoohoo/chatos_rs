import type {
  RemoteConnection,
  RemoteCredentialFileKind,
  RemoteCredentialFileSelection,
  RemoteConnectionInput,
  RemoteConnectionTestResult,
} from "../domain/remoteConnection";

export interface RemoteConnectionScope {
  ownerUserId: string;
}

export interface SavedRemoteConnectionScope extends RemoteConnectionScope {
  connectionId: string;
}

export interface CreateRemoteConnectionRequest extends RemoteConnectionScope {
  presentationId: string;
  input: RemoteConnectionInput;
}

export interface UpdateRemoteConnectionRequest extends SavedRemoteConnectionScope {
  presentationId: string;
  expectedVersion: number;
  input: RemoteConnectionInput;
}

export interface DeleteRemoteConnectionRequest extends SavedRemoteConnectionScope {
  expectedVersion: number;
}

export interface TestRemoteConnectionDraftRequest extends CreateRemoteConnectionRequest {
  credentialReferenceId: string | null;
  verificationCode: string | null;
}

export interface TestSavedRemoteConnectionRequest extends SavedRemoteConnectionScope {
  verificationCode: string | null;
}

export interface RemoteConnectionPort {
  list(request: RemoteConnectionScope): Promise<RemoteConnection[]>;
  get(request: SavedRemoteConnectionScope): Promise<RemoteConnection | null>;
  create(request: CreateRemoteConnectionRequest): Promise<RemoteConnection>;
  update(request: UpdateRemoteConnectionRequest): Promise<RemoteConnection>;
  delete(request: DeleteRemoteConnectionRequest): Promise<string>;
  testDraft(request: TestRemoteConnectionDraftRequest): Promise<RemoteConnectionTestResult>;
  testSaved(request: TestSavedRemoteConnectionRequest): Promise<RemoteConnectionTestResult>;
  pickCredentialFile(
    request: RemoteConnectionScope & { presentationId: string; kind: RemoteCredentialFileKind },
  ): Promise<RemoteCredentialFileSelection | null>;
  discardCredentialFileSelections(
    request: RemoteConnectionScope & { presentationId: string },
  ): Promise<void>;
}
