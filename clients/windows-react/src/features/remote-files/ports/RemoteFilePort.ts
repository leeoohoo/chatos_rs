import type {
  RemoteDirectoryResult,
  RemoteDownloadResponse,
  RemoteFileMutationResponse,
  RemoteFileOperationInput,
  RemoteLocalFileSelection,
} from "../domain/remoteFiles";

interface PresentationScope {
  ownerUserId: string;
  presentationId: string;
}

interface OperationScope {
  ownerUserId: string;
  input: RemoteFileOperationInput;
}

export interface RemoteFilePort {
  listDirectory(request: OperationScope & { path: string | null }): Promise<RemoteDirectoryResult>;
  createDirectory(request: OperationScope & { parentPath: string; name: string }): Promise<RemoteFileMutationResponse>;
  renameEntry(request: OperationScope & { path: string; newName: string }): Promise<RemoteFileMutationResponse>;
  deleteEntry(request: OperationScope & { path: string; recursively: boolean }): Promise<RemoteFileMutationResponse>;
  pickUploadFile(request: PresentationScope): Promise<RemoteLocalFileSelection | null>;
  pickDownloadDestination(
    request: PresentationScope & { suggestedFileName: string },
  ): Promise<RemoteLocalFileSelection | null>;
  uploadFile(request: OperationScope & PresentationScope & {
    selectionId: string;
    remoteDirectory: string;
    overwrite: boolean;
  }): Promise<RemoteFileMutationResponse>;
  downloadFile(request: OperationScope & PresentationScope & {
    selectionId: string;
    remotePath: string;
    overwrite: boolean;
  }): Promise<RemoteDownloadResponse>;
  discardSelections(request: PresentationScope): Promise<void>;
}
