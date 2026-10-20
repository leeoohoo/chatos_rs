import { invoke } from "@tauri-apps/api/core";
import {
  remoteDirectoryResultSchema,
  remoteDownloadResponseSchema,
  remoteFileMutationResponseSchema,
  remoteFileOperationInputSchema,
  remoteLocalFileSelectionSchema,
} from "../domain/remoteFiles";
import type { RemoteFilePort } from "../ports/RemoteFilePort";

export class TauriRemoteFileAdapter implements RemoteFilePort {
  async listDirectory(request: Parameters<RemoteFilePort["listDirectory"]>[0]) {
    return remoteDirectoryResultSchema.parse(await invoke("list_remote_directory", {
      ...request,
      input: remoteFileOperationInputSchema.parse(request.input),
    }));
  }

  async createDirectory(request: Parameters<RemoteFilePort["createDirectory"]>[0]) {
    return remoteFileMutationResponseSchema.parse(await invoke("create_remote_directory", {
      ...request,
      input: remoteFileOperationInputSchema.parse(request.input),
    }));
  }

  async renameEntry(request: Parameters<RemoteFilePort["renameEntry"]>[0]) {
    return remoteFileMutationResponseSchema.parse(await invoke("rename_remote_entry", {
      ...request,
      input: remoteFileOperationInputSchema.parse(request.input),
    }));
  }

  async deleteEntry(request: Parameters<RemoteFilePort["deleteEntry"]>[0]) {
    return remoteFileMutationResponseSchema.parse(await invoke("delete_remote_entry", {
      ...request,
      input: remoteFileOperationInputSchema.parse(request.input),
    }));
  }

  async pickUploadFile(request: Parameters<RemoteFilePort["pickUploadFile"]>[0]) {
    return remoteLocalFileSelectionSchema.nullable().parse(
      await invoke("pick_remote_upload_file", { ...request }),
    );
  }

  async pickDownloadDestination(
    request: Parameters<RemoteFilePort["pickDownloadDestination"]>[0],
  ) {
    return remoteLocalFileSelectionSchema.nullable().parse(
      await invoke("pick_remote_download_destination", { ...request }),
    );
  }

  async uploadFile(request: Parameters<RemoteFilePort["uploadFile"]>[0]) {
    return remoteFileMutationResponseSchema.parse(await invoke("upload_remote_file", {
      ...request,
      input: remoteFileOperationInputSchema.parse(request.input),
    }));
  }

  async downloadFile(request: Parameters<RemoteFilePort["downloadFile"]>[0]) {
    return remoteDownloadResponseSchema.parse(await invoke("download_remote_file", {
      ...request,
      input: remoteFileOperationInputSchema.parse(request.input),
    }));
  }

  async discardSelections(request: Parameters<RemoteFilePort["discardSelections"]>[0]) {
    await invoke("discard_remote_file_selections", { ...request });
  }
}
