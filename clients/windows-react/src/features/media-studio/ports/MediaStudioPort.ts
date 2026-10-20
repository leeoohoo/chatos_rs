import type {
  MediaStudioGenerationOutcome,
  MediaStudioGenerationRequest,
  MediaStudioHistoryCursor,
  MediaStudioHistoryPage,
  MediaStudioModel,
  MediaStudioReferenceImportResult,
} from "../domain/mediaStudio";

export interface MediaStudioPresentationScope {
  ownerUserId: string;
  presentationId: string;
}

export interface MediaStudioHistoryRequest {
  ownerUserId: string;
  after?: MediaStudioHistoryCursor | null;
  limit?: number | null;
}

export interface MediaStudioHistoryReference {
  recordId: string;
  contentToken: string;
}

export interface MediaStudioPort {
  listModels(ownerUserId: string): Promise<MediaStudioModel[]>;
  loadImageHistory(request: MediaStudioHistoryRequest): Promise<MediaStudioHistoryPage>;
  pickReferenceImages(
    request: MediaStudioPresentationScope,
  ): Promise<MediaStudioReferenceImportResult>;
  stageHistoryReferences(
    request: MediaStudioPresentationScope & { assets: MediaStudioHistoryReference[] },
  ): Promise<MediaStudioReferenceImportResult>;
  discardReferenceImages(
    request: MediaStudioPresentationScope & { referenceIds: string[] },
  ): Promise<void>;
  discardPresentation(request: MediaStudioPresentationScope): Promise<void>;
  generateImage(request: {
    ownerUserId: string;
    request: MediaStudioGenerationRequest;
  }): Promise<MediaStudioGenerationOutcome>;
}
