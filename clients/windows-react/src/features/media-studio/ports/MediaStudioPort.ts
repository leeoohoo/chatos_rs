import type {
  MediaStudioGenerationOutcome,
  MediaStudioGenerationRequest,
  MediaStudioHistoryCursor,
  MediaStudioHistoryPage,
  MediaStudioModel,
  MediaStudioReferenceImportResult,
  MediaStudioVideoAudioDraft,
  MediaStudioVideoGenerationRequest,
  MediaStudioVideoHistoryCursor,
  MediaStudioVideoHistoryPage,
  MediaStudioVideoJobSnapshot,
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
  loadVideoHistory(request: {
    ownerUserId: string;
    after?: MediaStudioVideoHistoryCursor | null;
    limit?: number | null;
  }): Promise<MediaStudioVideoHistoryPage>;
  pickReferenceImages(
    request: MediaStudioPresentationScope,
  ): Promise<MediaStudioReferenceImportResult>;
  stageHistoryReferences(
    request: MediaStudioPresentationScope & { assets: MediaStudioHistoryReference[] },
  ): Promise<MediaStudioReferenceImportResult>;
  discardReferenceImages(
    request: MediaStudioPresentationScope & { referenceIds: string[] },
  ): Promise<void>;
  pickVideoReferenceAudio(
    request: MediaStudioPresentationScope,
  ): Promise<MediaStudioVideoAudioDraft | null>;
  discardVideoReferenceAudio(
    request: MediaStudioPresentationScope & { referenceAudioId: string },
  ): Promise<void>;
  discardPresentation(request: MediaStudioPresentationScope): Promise<void>;
  generateImage(request: {
    ownerUserId: string;
    request: MediaStudioGenerationRequest;
  }): Promise<MediaStudioGenerationOutcome>;
  startVideoGeneration(request: {
    ownerUserId: string;
    request: MediaStudioVideoGenerationRequest;
  }): Promise<MediaStudioVideoJobSnapshot>;
  pollVideoGeneration(
    request: MediaStudioPresentationScope & { jobId: string; clientRequestId: string },
  ): Promise<MediaStudioVideoJobSnapshot>;
  cancelVideoGeneration(
    request: MediaStudioPresentationScope & { jobId: string },
  ): Promise<void>;
}
