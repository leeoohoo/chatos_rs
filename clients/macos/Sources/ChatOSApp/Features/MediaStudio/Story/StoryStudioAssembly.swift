import ChatOSAgentRuntime
import ChatOSCore
import ChatOSMediaWorkerClient

/// macOS composition root for Story Studio. Concrete stores, providers and native
/// adapters are assembled here and never retained by the presentation ViewModel.
@MainActor
extension StoryStudioViewModel {
    convenience init(
        media: any MediaGenerationServicing,
        planner: (any StoryPlanningServicing)? = nil,
        store: StoryProjectStore = StoryProjectStore(),
        videoJobStore: MediaStudioVideoJobStore? = nil,
        projectCatalog: (any StoryProjectCatalogLoading)? = nil,
        projectMutation: (any StoryProjectMutationPersisting)? = nil,
        agentServices: (any AgentServiceProviding)? = nil,
        storyPlanning: (any StoryPlanningExecuting)? = nil,
        storyHistory: (any StoryHistoryLoading)? = nil,
        storyMediaBatch: (any StoryMediaBatchExecuting)? = nil,
        storyMediaAccess: (any StoryMediaAccessResolving)? = nil,
        storyImageImport: (any StoryImageImporting)? = nil,
        storyAssetImageGeneration: (any StoryAssetImageGenerating)? = nil,
        storyFrameImageGeneration: (any StoryFrameImageGenerating)? = nil,
        storyVideoGeneration: (any StoryVideoGenerating)? = nil,
        storyVideoTailExtraction: (any StoryVideoTailExtracting)? = nil,
        mediaWorker: any MediaWorkerImageNormalizing = MediaWorkerProcessClient.shared,
        agentSettings: AgentSettingsStore = .init()
    ) {
        // The top-level Media Studio passes its shared store explicitly. Standalone Story
        // compositions (including isolated tests) keep durable jobs beside their Story root.
        let resolvedVideoJobStore = videoJobStore ?? MediaStudioVideoJobStore(
            root: store.root.appendingPathComponent("MediaJobs", isDirectory: true)
        )
        let resolvedAgentServices = agentServices ?? (planner as? any AgentServiceProviding)
        let resolvedStoryPlanning: StoryPlanningUseCase?
        if let storyPlanning {
            resolvedStoryPlanning = StoryPlanningUseCase(executor: storyPlanning)
        } else if let resolvedAgentServices {
            resolvedStoryPlanning = StoryPlanningUseCase(executor: StoryPlanningAgentAdapter(
                store: store,
                services: resolvedAgentServices,
                settings: agentSettings
            ))
        } else {
            resolvedStoryPlanning = nil
        }

        self.init(
            planner: planner,
            projectCatalog: StoryProjectCatalogUseCase(catalog: projectCatalog ?? store),
            projectMutation: StoryProjectMutationUseCase(
                persistence: projectMutation ?? store
            ),
            storyPlanning: resolvedStoryPlanning,
            storyHistory: StoryHistoryUseCase(
                history: storyHistory ?? StoryHistoryAdapter(
                    store: store,
                    videoJobStore: resolvedVideoJobStore
                )
            ),
            storyMediaBatch: StoryMediaBatchUseCase(
                executor: storyMediaBatch ?? StoryMediaBatchExecutionAdapter(
                    store: store,
                    media: media,
                    mediaWorker: mediaWorker,
                    videoJobStore: resolvedVideoJobStore
                )
            ),
            storyMediaAccess: StoryMediaAccessUseCase(
                resolver: storyMediaAccess ?? StoryMediaAccessAdapter(store: store)
            ),
            storyImageImport: StoryImageImportUseCase(
                importer: storyImageImport ?? StoryImageImportAdapter(
                    store: store,
                    mediaWorker: mediaWorker
                )
            ),
            storyAssetImageGeneration: StoryAssetImageGenerationUseCase(
                generator: storyAssetImageGeneration ?? StoryAssetImageGenerationAdapter(
                    store: store,
                    media: media,
                    mediaWorker: mediaWorker
                )
            ),
            storyFrameImageGeneration: StoryFrameImageGenerationUseCase(
                generator: storyFrameImageGeneration ?? StoryFrameImageGenerationAdapter(
                    store: store,
                    media: media,
                    mediaWorker: mediaWorker
                )
            ),
            storyVideoGeneration: StoryVideoGenerationUseCase(
                generator: storyVideoGeneration ?? StoryVideoGenerationAdapter(
                    store: store,
                    media: media,
                    videoJobStore: resolvedVideoJobStore
                )
            ),
            storyVideoTailExtraction: StoryVideoTailExtractionUseCase(
                extractor: storyVideoTailExtraction
                    ?? StoryVideoTailExtractionAdapter(store: store)
            )
        )
    }
}
