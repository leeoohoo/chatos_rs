import { invoke } from "@tauri-apps/api/core";
import {
  remoteConnectionIdentifierSchema,
  remoteConnectionInputSchema,
  remoteConnectionListSchema,
  remoteConnectionSchema,
  remoteConnectionTestResultSchema,
  remoteCredentialFileSelectionSchema,
} from "../domain/remoteConnection";
import type {
  CreateRemoteConnectionRequest,
  DeleteRemoteConnectionRequest,
  RemoteConnectionPort,
  RemoteConnectionScope,
  SavedRemoteConnectionScope,
  TestRemoteConnectionDraftRequest,
  TestSavedRemoteConnectionRequest,
  UpdateRemoteConnectionRequest,
} from "../ports/RemoteConnectionPort";

export class TauriRemoteConnectionAdapter implements RemoteConnectionPort {
  async list(request: RemoteConnectionScope) {
    return remoteConnectionListSchema.parse(
      await invoke("list_remote_connections", { ...request }),
    );
  }

  async get(request: SavedRemoteConnectionScope) {
    return remoteConnectionSchema.nullable().parse(
      await invoke("get_remote_connection", { ...request }),
    );
  }

  async create(request: CreateRemoteConnectionRequest) {
    const input = remoteConnectionInputSchema.parse(request.input);
    return remoteConnectionSchema.parse(
      await invoke("create_remote_connection", {
        ownerUserId: request.ownerUserId,
        presentationId: request.presentationId,
        input,
      }),
    );
  }

  async update(request: UpdateRemoteConnectionRequest) {
    const input = remoteConnectionInputSchema.parse(request.input);
    return remoteConnectionSchema.parse(
      await invoke("update_remote_connection", { ...request, input }),
    );
  }

  async delete(request: DeleteRemoteConnectionRequest) {
    return remoteConnectionIdentifierSchema.parse(
      await invoke("delete_remote_connection", { ...request }),
    );
  }

  async testDraft(request: TestRemoteConnectionDraftRequest) {
    const input = remoteConnectionInputSchema.parse(request.input);
    return remoteConnectionTestResultSchema.parse(
      await invoke("test_remote_connection_draft", { ...request, input }),
    );
  }

  async testSaved(request: TestSavedRemoteConnectionRequest) {
    return remoteConnectionTestResultSchema.parse(
      await invoke("test_saved_remote_connection", { ...request }),
    );
  }

  async pickCredentialFile(
    request: Parameters<RemoteConnectionPort["pickCredentialFile"]>[0],
  ) {
    return remoteCredentialFileSelectionSchema.nullable().parse(
      await invoke("pick_remote_credential_file", { ...request }),
    );
  }

  async discardCredentialFileSelections(
    request: Parameters<RemoteConnectionPort["discardCredentialFileSelections"]>[0],
  ) {
    await invoke("discard_remote_credential_file_selections", { ...request });
  }
}
