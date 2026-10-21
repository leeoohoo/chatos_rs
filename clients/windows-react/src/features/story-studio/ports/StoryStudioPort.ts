import type {
  StoryProject,
  StoryProjectCursor,
  StoryProjectPage,
  StoryVideoGenerationOutcome,
} from "../domain/storyStudio";

export interface StoryPresentationScope {
  ownerUserId: string;
  presentationId: string;
}

export interface StoryStudioPort {
  loadProjects(request: StoryPresentationScope & {
    after?: StoryProjectCursor | null;
    limit?: number | null;
  }): Promise<StoryProjectPage>;
  loadProject(request: StoryPresentationScope & { projectId: string }): Promise<StoryProject>;
  createProject(request: StoryPresentationScope & {
    title: string;
    description: string;
    textModelId: string;
    imageModelId: string;
    videoModelId: string;
  }): Promise<StoryProject>;
  updateSource(request: StoryPresentationScope & {
    projectId: string;
    baseFingerprint: string;
    source: string;
    style: string;
    ratio: string;
  }): Promise<StoryProject>;
  startPlanning(request: StoryPresentationScope & {
    projectId: string;
    stage: "outline" | "refine";
    targetIds: string[];
    userIdeas: string;
  }): Promise<StoryProject | null>;
  generateAssetImage(request: StoryPresentationScope & {
    projectId: string;
    baseFingerprint: string;
    resourceId: string;
    userIdeas: string;
  }): Promise<StoryProject>;
  generateFrameImage(request: StoryPresentationScope & {
    projectId: string;
    baseFingerprint: string;
    segmentId: string;
    frameRole: "first" | "last";
    referenceResourceIds: string[];
    userIdeas: string;
  }): Promise<StoryProject>;
  confirmLatestResourceImages(request: StoryPresentationScope & {
    projectId: string;
    baseFingerprint: string;
    resourceIds: string[];
  }): Promise<StoryProject>;
  confirmLatestFrames(request: StoryPresentationScope & {
    projectId: string;
    baseFingerprint: string;
    segmentId: string;
    useConfirmedLastFrameForVideo: boolean;
  }): Promise<StoryProject>;
  startVideo(request: StoryPresentationScope & {
    projectId: string;
    baseFingerprint: string;
    segmentId: string;
    size: "768P" | "2K";
    userIdeas: string;
  }): Promise<StoryVideoGenerationOutcome>;
  pollVideo(request: StoryPresentationScope & {
    projectId: string;
    segmentId: string;
  }): Promise<StoryVideoGenerationOutcome>;
  discardPresentation(request: StoryPresentationScope): Promise<void>;
}
