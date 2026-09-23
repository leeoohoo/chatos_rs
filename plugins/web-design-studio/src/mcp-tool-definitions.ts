import {
  policy,
  componentStyleSchema,
  componentFrameProperties,
  webDesignComponentSchema,
  patchOperationSchema,
  generationDesignIntentSchema,
  generationStepSchema,
  generationArtifactSchema,
  simpleSceneFrameSchema,
  simpleSceneNodeSchema,
  generationSceneOperationSchema,
  progressiveStepExecutionProperties,
  visualRectSchema
} from './mcp-tool-schemas.js';
import { UI_LIBRARIES } from './ui-libraries.js';
import { WEB_DESIGN_BLOCK_PRESETS, WEB_DESIGN_PAGE_TEMPLATES } from './component-library.js';

export const TOOL_DEFINITIONS_BASE = [
  {
    name: 'web_design_get_active_context',
    description: 'Read the immutable ChatOS project scope, active document/page hints, current selection, pending human requests, resumable generation plan, and delivery gate. Follow the returned nextAction before unrelated implementation work or task completion. This tool accepts no projectId.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
    _meta: policy
  },
  {
    name: 'web_design_plan_site',
    description: 'Create or revise only the semantic artboard inventory and site objective. This planning-only call never creates visible Scene content and is not a deliverable. In the same task run, immediately continue with the returned deliveryGate.requiredNextAction.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedPlanRevision: { type: 'integer', minimum: 0 },
        planId: { type: 'string', minLength: 1, maxLength: 160 },
        mode: { type: 'string', enum: ['guided', 'auto-current-page', 'review-sensitive'], default: 'auto-current-page' },
        objective: { type: 'string', minLength: 1, maxLength: 12000 },
        audience: { type: 'array', minItems: 1, maxItems: 40, items: { type: 'string', minLength: 1, maxLength: 1000 } },
        pages: {
          type: 'array', minItems: 1, maxItems: 100,
          items: {
            type: 'object',
            properties: {
              pageId: { type: 'string', minLength: 1, maxLength: 160 },
              name: { type: 'string', minLength: 1, maxLength: 240 },
              purpose: { type: 'string', minLength: 1, maxLength: 4000 }
            },
            required: ['pageId', 'name', 'purpose'], additionalProperties: false
          }
        }
      },
      required: ['documentId', 'objective', 'audience', 'pages'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_plan_page',
    description: 'Plan one semantic artboard only: save its visual direction, content hierarchy, acceptance criteria, and bounded step dependency graph. It does not create visible content; immediately continue with the returned deliveryGate.requiredNextAction.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedPlanRevision: { type: 'integer', minimum: 0 },
        pageId: { type: 'string', minLength: 1, maxLength: 160 },
        design: generationDesignIntentSchema,
        steps: { type: 'array', minItems: 1, maxItems: 64, items: generationStepSchema }
      },
      required: ['documentId', 'expectedPlanRevision', 'pageId', 'design', 'steps'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_get_plan',
    description: 'Read a compact generation-plan summary with stable IDs, current page/step, delivery gate, status counts, and exactly one required next action.',
    inputSchema: {
      type: 'object', properties: { documentId: { type: 'string', minLength: 1, maxLength: 128 } },
      required: ['documentId'], additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_start_page',
    description: 'Start exactly one planned artboard and ensure its empty canonical Scene root frame exists. The root alone is still visually empty and cannot satisfy delivery; capture it and run the returned visual Step next.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedPlanRevision: { type: 'integer', minimum: 0 },
        pageId: { type: 'string', minLength: 1, maxLength: 160 },
        viewportWidth: { type: 'integer', minimum: 240, maximum: 10000, default: 1440 }
      },
      required: ['documentId', 'expectedPlanRevision', 'pageId'], additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_capture_page',
    description: 'Render one Scene v2 page with Chromium and return the actual PNG plus persistent snapshot, layout, grounding, and calibration artifacts for the exact Scene revision.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        pageId: { type: 'string', minLength: 1, maxLength: 160 },
        viewportWidth: { type: 'integer', minimum: 240, maximum: 10000, default: 1440 }
      },
      required: ['documentId', 'pageId'], additionalProperties: false
    },
    _meta: { ...policy, 'chatos/toolResultMaxChars': 500_000 }
  },
  {
    name: 'web_design_capture_region',
    description: 'Render and return a PNG crop from one page using either a stable Scene nodeId or an explicit page-space rectangle. Grounding coordinates are relative to the returned crop.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        pageId: { type: 'string', minLength: 1, maxLength: 160 },
        viewportWidth: { type: 'integer', minimum: 240, maximum: 10000, default: 1440 },
        nodeId: { type: 'string', minLength: 1, maxLength: 160 },
        rect: visualRectSchema,
        padding: { type: 'number', minimum: 0, maximum: 1000, default: 16 }
      },
      required: ['documentId', 'pageId'],
      anyOf: [{ required: ['nodeId'] }, { required: ['rect'] }],
      additionalProperties: false
    },
    _meta: { ...policy, 'chatos/toolResultMaxChars': 500_000 }
  },
  {
    name: 'web_design_prepare_annotation_task',
    description: 'Prepare one open human Scene annotation as a revision-bound AI task and return a fresh PNG crop with stable-node grounding. Use this before editing the annotated target.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        nodeId: { type: 'string', minLength: 1, maxLength: 160 },
        annotationId: { type: 'string', minLength: 1, maxLength: 160 },
        viewportWidth: { type: 'integer', minimum: 240, maximum: 10000, default: 1440 },
        dependencyNodeIds: { type: 'array', maxItems: 64, uniqueItems: true, items: { type: 'string', minLength: 1, maxLength: 160 } },
        padding: { type: 'number', minimum: 0, maximum: 1000, default: 24 }
      },
      required: ['documentId', 'nodeId', 'annotationId'], additionalProperties: false
    },
    _meta: { ...policy, 'chatos/toolResultMaxChars': 500_000 }
  },
  {
    name: 'web_design_get_visual_grounding',
    description: 'Read the persistent mapping from stable Scene node IDs to rectangles for a previously captured PNG and return that same image for visual grounding.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        artifactId: { type: 'string', minLength: 1, maxLength: 160 }
      },
      required: ['documentId', 'artifactId'], additionalProperties: false
    },
    _meta: { ...policy, 'chatos/toolResultMaxChars': 500_000 }
  },
  {
    name: 'web_design_compare_snapshots',
    description: 'Compare two persisted snapshots of the same page and viewport. Returns before, after, and highlighted Diff PNGs with changed regions and affected stable node IDs.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        beforeArtifactId: { type: 'string', minLength: 1, maxLength: 160 },
        afterArtifactId: { type: 'string', minLength: 1, maxLength: 160 }
      },
      required: ['documentId', 'beforeArtifactId', 'afterArtifactId'], additionalProperties: false
    },
    _meta: { ...policy, 'chatos/toolResultMaxChars': 500_000 }
  },
  {
    name: 'web_design_inspect_at_point',
    description: 'Inspect a point in a captured PNG and return selectable stable Scene node candidates ordered from the smallest visible hit, including their ancestor path and the source image.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        artifactId: { type: 'string', minLength: 1, maxLength: 160 },
        x: { type: 'number', minimum: 0, maximum: 10000 },
        y: { type: 'number', minimum: 0, maximum: 50000 },
        limit: { type: 'integer', minimum: 1, maximum: 50, default: 12 }
      },
      required: ['documentId', 'artifactId', 'x', 'y'], additionalProperties: false
    },
    _meta: { ...policy, 'chatos/toolResultMaxChars': 500_000 }
  },
  {
    name: 'web_design_query_scene',
    description: 'Read a bounded flat set of editable nodes from exactly one artboard chosen from get_active_context.artboardDirectory. Never loads several artboards into model context. Containers never recursively repeat descendants.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        artboardId: { type: 'string', minLength: 1, maxLength: 160 },
        query: {
          type: 'object',
          properties: {
            ids: { type: 'array', minItems: 1, maxItems: 256, uniqueItems: true, items: { type: 'string', minLength: 1, maxLength: 160 } },
            types: { type: 'array', minItems: 1, maxItems: 10, uniqueItems: true, items: { type: 'string', enum: ['section', 'frame', 'group', 'text', 'shape', 'media', 'library-instance', 'component-main', 'component-set', 'component-instance'] } },
            roles: { type: 'array', minItems: 1, maxItems: 100, uniqueItems: true, items: { type: 'string', minLength: 1, maxLength: 160 } },
            name: {
              type: 'object',
              properties: {
                equals: { type: 'string', minLength: 1, maxLength: 240 },
                contains: { type: 'string', minLength: 1, maxLength: 240 },
                startsWith: { type: 'string', minLength: 1, maxLength: 240 },
                caseSensitive: { type: 'boolean' }
              },
              oneOf: [{ required: ['equals'] }, { required: ['contains'] }, { required: ['startsWith'] }],
              additionalProperties: false
            },
            visible: { type: 'boolean' }, locked: { type: 'boolean' }, aiEditable: { type: 'boolean' },
            hasLockedFields: { type: 'boolean' }, limit: { type: 'integer', minimum: 1, maximum: 256, default: 100 }
          },
          additionalProperties: false
        }
      },
      required: ['documentId', 'artboardId'], additionalProperties: false
    },
    _meta: { ...policy, 'chatos/toolResultMaxChars': 500_000 }
  },
  {
    name: 'web_design_edit_scene',
    description: 'Apply one atomic commandJson action inside exactly one artboard chosen from get_active_context.artboardDirectory. The server validates the decoded command and rejects cross-artboard edits. Use for focused revision-safe adjustments, then capture that same artboard.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        artboardId: { type: 'string', minLength: 1, maxLength: 160 },
        transactionId: { type: 'string', minLength: 1, maxLength: 160 },
        expectedRevision: { type: 'integer', minimum: 0 },
        reason: { type: 'string', minLength: 1, maxLength: 240 },
        commandJson: { type: 'string', minLength: 2, maxLength: 262144, description: 'JSON object encoded as text. Load web-design-scene-building for command formats.' }
      },
      required: ['documentId', 'artboardId', 'transactionId', 'expectedRevision', 'commandJson'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_run_next_step',
    description: 'Create and mechanically verify one Candidate Transaction for the next ready step. Supply current-revision page captures; the plugin returns real Candidate and Diff images and waits for explicit visual review before commit.',
    inputSchema: {
      type: 'object', properties: progressiveStepExecutionProperties,
      required: ['documentId', 'expectedPlanRevision', 'idempotencyKey', 'transactionId', 'operations', 'visualInputs'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_retry_step',
    description: 'Retry one explicit failed, rejected, stale, or rolled-back step using fresh current-revision visual inputs. Accepted steps are never replayed.',
    inputSchema: {
      type: 'object',
      properties: { ...progressiveStepExecutionProperties, stepId: { type: 'string', minLength: 1, maxLength: 160 } },
      required: ['documentId', 'expectedPlanRevision', 'stepId', 'idempotencyKey', 'transactionId', 'operations', 'visualInputs'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_repair_step',
    description: 'Create one targeted repair Candidate for a failed Step using its recorded visual issue IDs and fresh current-revision visual evidence. It cannot broaden the Step target.',
    inputSchema: {
      type: 'object',
      properties: { ...progressiveStepExecutionProperties, stepId: { type: 'string', minLength: 1, maxLength: 160 } },
      required: ['documentId', 'expectedPlanRevision', 'stepId', 'idempotencyKey', 'transactionId', 'operations', 'visualInputs'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_execute_step',
    description: 'Prepare one visible Scene Candidate from operationsJson for the next or named Step. The plugin validates decoded operations, captures every required viewport, chooses first-run/retry/repair behavior, creates IDs, renders Candidate and Diff PNGs, and waits for explicit visual review.',
    inputSchema: {
      type: 'object', properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedPlanRevision: { type: 'integer', minimum: 0 },
        stepId: { type: 'string', minLength: 1, maxLength: 160 },
        requestId: { type: 'string', minLength: 1, maxLength: 160 },
        operationsJson: { type: 'string', minLength: 2, maxLength: 262144, description: 'JSON array encoded as text. Load web-design-scene-building for insert-simple-tree and focused operation formats.' }
      },
      required: ['documentId', 'expectedPlanRevision', 'operationsJson'],
      additionalProperties: false
    },
    _meta: { ...policy, 'chatos/toolResultMaxChars': 500_000 }
  },
  {
    name: 'web_design_control_plan',
    description: 'Apply one lifecycle decision to the active Plan. Accept commits only the reviewed Candidate and automatically completes an accepted handoff page; reject/skip/rollback/pause/resume/start-page are revision checked. This tool never generates visual content.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedPlanRevision: { type: 'integer', minimum: 0 },
        action: { type: 'string', enum: ['accept', 'reject', 'skip', 'rollback', 'pause', 'resume', 'start-page'] },
        pageId: { type: 'string', minLength: 1, maxLength: 160 },
        stepId: { type: 'string', minLength: 1, maxLength: 160 },
        attemptId: { type: 'string', minLength: 1, maxLength: 160 },
        reason: { type: 'string', minLength: 1, maxLength: 12000 },
        approveSoftProtectionConflicts: { type: 'boolean', default: false },
        viewportWidth: { type: 'integer', minimum: 240, maximum: 10000, default: 1440 }
      },
      required: ['documentId', 'expectedPlanRevision', 'action'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_inspect_step',
    description: 'Inspect one generation attempt with its target, Candidate Diff artifacts, screenshots, quality report, issue IDs, and protected-field conflicts.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        stepId: { type: 'string', minLength: 1, maxLength: 160 },
        attemptId: { type: 'string', minLength: 1, maxLength: 160 }
      },
      required: ['documentId', 'stepId'], additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_accept_step',
    description: 'Commit one already validated Candidate Transaction. Soft-protected human fields require an explicit approval flag; no other step is executed.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedPlanRevision: { type: 'integer', minimum: 0 },
        stepId: { type: 'string', minLength: 1, maxLength: 160 },
        attemptId: { type: 'string', minLength: 1, maxLength: 160 },
        approveSoftProtectionConflicts: { type: 'boolean', default: false }
      },
      required: ['documentId', 'expectedPlanRevision', 'stepId', 'attemptId'], additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_reject_step',
    description: 'Reject and discard one reviewed Candidate while preserving the formal Scene and all accepted steps.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 }, expectedPlanRevision: { type: 'integer', minimum: 0 },
        stepId: { type: 'string', minLength: 1, maxLength: 160 }, attemptId: { type: 'string', minLength: 1, maxLength: 160 },
        reason: { type: 'string', minLength: 1, maxLength: 12000 }
      },
      required: ['documentId', 'expectedPlanRevision', 'stepId', 'attemptId', 'reason'], additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_skip_step',
    description: 'Explicitly skip one ready optional Step on the active page. Required design, Design Gate, and handoff steps cannot be skipped.',
    inputSchema: {
      type: 'object', properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 }, expectedPlanRevision: { type: 'integer', minimum: 0 },
        stepId: { type: 'string', minLength: 1, maxLength: 160 }
      }, required: ['documentId', 'expectedPlanRevision', 'stepId'], additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_rollback_step',
    description: 'Roll back one accepted Step only when its exact transaction is still the latest Scene change. This restores the prior Scene and marks dependent steps stale instead of overwriting later human work.',
    inputSchema: {
      type: 'object', properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 }, expectedPlanRevision: { type: 'integer', minimum: 0 },
        stepId: { type: 'string', minLength: 1, maxLength: 160 }
      }, required: ['documentId', 'expectedPlanRevision', 'stepId'], additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_complete_page',
    description: 'Complete only the active artboard after its handoff Step and every required visual Step are accepted. Only completed artboards may be implemented in product source code. Continue the plan before claiming the full requested design scope is complete.',
    inputSchema: {
      type: 'object', properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 }, expectedPlanRevision: { type: 'integer', minimum: 0 },
        pageId: { type: 'string', minLength: 1, maxLength: 160 }
      }, required: ['documentId', 'expectedPlanRevision', 'pageId'], additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_pause_plan',
    description: 'Persistently pause the active page at a safe boundary. An active generating or reviewed step must be resolved first.',
    inputSchema: {
      type: 'object', properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 }, expectedPlanRevision: { type: 'integer', minimum: 0 }
      }, required: ['documentId', 'expectedPlanRevision'], additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_resume_plan',
    description: 'Resume the single paused page without starting a new page or executing a generation step.',
    inputSchema: {
      type: 'object', properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 }, expectedPlanRevision: { type: 'integer', minimum: 0 }
      }, required: ['documentId', 'expectedPlanRevision'], additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_list_documents',
    description: 'List editable website design documents in the current program-injected ChatOS scope.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
    _meta: policy
  },
  {
    name: 'web_design_create_document',
    description: 'Create an empty AI-first design workspace in the current program-injected ChatOS scope. This is not visible design output: immediately plan the site, plan one artboard, start it, and accept a visible Candidate before unrelated implementation work.',
    inputSchema: {
      type: 'object',
      properties: {
        title: { type: 'string', minLength: 1, maxLength: 240 }
      },
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_get_document',
    description: 'Read the complete website design document. This is a recovery/global-inspection tool and can be large. For normal AI work, read web_design_get_document_outline first, then web_design_get_page for only the page being edited.',
    inputSchema: {
      type: 'object',
      properties: { documentId: { type: 'string', minLength: 1, maxLength: 128 } },
      required: ['documentId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_get_document_outline',
    description: 'Read sparse Figma-like document metadata: pages, root layers, type distribution, component-system usage, and quality signals. Call this before opening or editing a design; it intentionally omits component content and full node payloads.',
    inputSchema: {
      type: 'object',
      properties: { documentId: { type: 'string', minLength: 1, maxLength: 128 } },
      required: ['documentId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_get_page',
    description: 'Read one editable page and its node tree only. Use this after web_design_get_document_outline and work one page at a time instead of loading the complete document. For a large page or focused edit, use web_design_get_node on the relevant semantic container.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        pageId: { type: 'string', minLength: 1, maxLength: 128 }
      },
      required: ['documentId', 'pageId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_get_node',
    description: 'Read one component/Frame and a bounded descendant subtree, plus its ancestor path. Use this Figma-like focused context for component-by-component inspection and revision without loading unrelated page nodes.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        componentId: { type: 'string', minLength: 1, maxLength: 128 },
        depth: { type: 'integer', minimum: 0, maximum: 8, default: 4 }
      },
      required: ['documentId', 'componentId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_get_catalog',
    description: 'Read a small catalog index. The default summary returns library and asset-kind counts only; request one kind for bounded library, section, template, or theme summaries. Use catalog search next instead of loading all design supply.',
    inputSchema: {
      type: 'object',
      properties: { kind: { type: 'string', enum: ['summary', 'libraries', 'sections', 'templates', 'themes'], default: 'summary' } },
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_search_catalog',
    description: 'Search one bounded catalog kind by intent or keyword. Component results are compact and require web_design_get_component_contract before insertion; section, template, and theme results are design references, not mandatory art direction.',
    inputSchema: {
      type: 'object',
      properties: {
        kind: { type: 'string', enum: ['components', 'sections', 'templates', 'themes'], default: 'components' },
        query: { type: 'string', maxLength: 200 },
        libraryId: { type: 'string', enum: UI_LIBRARIES.map((library) => library.id) },
        category: { type: 'string', maxLength: 120 },
        includeDeprecated: { type: 'boolean', default: false },
        limit: { type: 'integer', minimum: 1, maximum: 50, default: 20 }
      },
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_search_components',
    description: 'Search a bounded component catalog by intent, label, category, or component name. Returns compact candidates only; call web_design_get_component_contract for the chosen component before inserting it.',
    inputSchema: {
      type: 'object',
      properties: {
        query: { type: 'string', maxLength: 200, description: 'Optional user intent or component keyword, for example pricing card, 表格, navigation, or upload.' },
        libraryId: { type: 'string', enum: UI_LIBRARIES.map((library) => library.id) },
        category: { type: 'string', maxLength: 120 },
        includeDeprecated: { type: 'boolean', default: false },
        limit: { type: 'integer', minimum: 1, maximum: 50, default: 20 }
      },
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_get_component_contract',
    description: 'Read one selected component contract with its supported variants, exact library binding template, default content and size, and editable slots. Do not invent library, component, variant, prop, or slot names.',
    inputSchema: {
      type: 'object',
      properties: {
        libraryId: { type: 'string', enum: UI_LIBRARIES.map((library) => library.id) },
        componentId: { type: 'string', minLength: 1, maxLength: 160 }
      },
      required: ['libraryId', 'componentId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_insert_section',
    description: 'Insert one production-ready responsive page section at the end of a page while preserving all existing components.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedRevision: { type: 'integer', minimum: 0 },
        pageId: { type: 'string', minLength: 1, maxLength: 128 },
        sectionId: { type: 'string', enum: WEB_DESIGN_BLOCK_PRESETS.map((section) => section.id) }
      },
      required: ['documentId', 'expectedRevision', 'pageId', 'sectionId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_apply_page_template',
    description: 'Replace one page with a complete editable responsive page template while preserving every other page.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedRevision: { type: 'integer', minimum: 0 },
        pageId: { type: 'string', minLength: 1, maxLength: 128 },
        templateId: { type: 'string', enum: WEB_DESIGN_PAGE_TEMPLATES.map((template) => template.id) }
      },
      required: ['documentId', 'expectedRevision', 'pageId', 'templateId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_replace_document',
    description: 'Replace a complete website design using optimistic revision control. Prefer focused patches.',
    inputSchema: {
      type: 'object',
      properties: {
        expectedRevision: { type: 'integer', minimum: 0 },
        document: { type: 'object' }
      },
      required: ['expectedRevision', 'document'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_apply_patch',
    description: 'Apply a small focused edit without replacing unrelated work. A call may touch components on only one page, contain at most 48 operations and 24 component insertions, and stay below 64 KB. Build one logical region at a time. If a call is too large, split it; never fall back to a large text node that simulates UI.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedRevision: { type: 'integer', minimum: 0 },
        operations: {
          type: 'array',
          minItems: 1,
          maxItems: 48,
          items: patchOperationSchema
        }
      },
      required: ['documentId', 'expectedRevision', 'operations'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_apply_node_batch',
    description: 'Insert or replace up to 24 editable nodes in one page and one logical region, using optimistic revision control. This is the preferred construction tool after reading the page. Every visual object must be a separate node; never encode a complete interface in text content.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedRevision: { type: 'integer', minimum: 0 },
        pageId: { type: 'string', minLength: 1, maxLength: 128 },
        regionName: { type: 'string', minLength: 1, maxLength: 120, description: 'Logical chunk such as Header, Sidebar navigation, Hero, Pricing grid, or Checkout form.' },
        components: { type: 'array', minItems: 1, maxItems: 24, items: webDesignComponentSchema }
      },
      required: ['documentId', 'expectedRevision', 'pageId', 'regionName', 'components'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_auto_layout',
    description: 'Apply a container\'s Flex row, Flex column, or Grid layout to its direct children for one responsive device, including justify distribution and Flex row wrapping.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedRevision: { type: 'integer', minimum: 0 },
        containerId: { type: 'string', minLength: 1, maxLength: 128 },
        device: { type: 'string', enum: ['desktop', 'tablet', 'mobile'] }
      },
      required: ['documentId', 'expectedRevision', 'containerId', 'device'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_sync_symbol_instances',
    description: 'Synchronize every instance of one reusable component while preserving instance-level content, style, or frame overrides.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedRevision: { type: 'integer', minimum: 0 },
        symbolId: { type: 'string', minLength: 1, maxLength: 128 }
      },
      required: ['documentId', 'expectedRevision', 'symbolId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_update_symbol_from_instance',
    description: 'Update a reusable component definition from one selected instance, then synchronize its other instances.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedRevision: { type: 'integer', minimum: 0 },
        componentId: { type: 'string', minLength: 1, maxLength: 128 }
      },
      required: ['documentId', 'expectedRevision', 'componentId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_export_html',
    description: 'Export one page or every page as standalone HTML using the selected responsive device layout.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        pageId: { type: 'string', minLength: 1, maxLength: 128 },
        device: { type: 'string', enum: ['desktop', 'tablet', 'mobile'], default: 'desktop' }
      },
      required: ['documentId'],
      additionalProperties: false
    },
    _meta: { ...policy, 'chatos/toolResultMaxChars': 500_000 }
  },
  {
    name: 'web_design_export_react',
    description: 'Export the complete multi-page design as a single React JSX component with client-side route navigation.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        device: { type: 'string', enum: ['desktop', 'tablet', 'mobile'], default: 'desktop' }
      },
      required: ['documentId'],
      additionalProperties: false
    },
    _meta: { ...policy, 'chatos/toolResultMaxChars': 500_000 }
  },
  {
    name: 'web_design_export_vue',
    description: 'Export the complete multi-page design as a single Vue SFC with client-side route navigation.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        device: { type: 'string', enum: ['desktop', 'tablet', 'mobile'], default: 'desktop' }
      },
      required: ['documentId'],
      additionalProperties: false
    },
    _meta: { ...policy, 'chatos/toolResultMaxChars': 500_000 }
  },
  {
    name: 'web_design_list_requests',
    description: 'List pending or all human design requests, including Scene v2 node annotations with stable node/page/revision context and the tool call needed to prepare visual evidence.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        includeResolved: { type: 'boolean', default: false }
      },
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_resolve_request',
    description: 'Mark a component-level AI request resolved after applying the requested design change.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedRevision: { type: 'integer', minimum: 0 },
        requestId: { type: 'string', minLength: 1, maxLength: 128 },
        resolution: { type: 'string', maxLength: 4000 }
      },
      required: ['documentId', 'expectedRevision', 'requestId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_validate',
    description: 'Validate structure and visual-delivery quality. Use draft mode after each page region, then handoff mode before completion. Handoff rejects empty/underbuilt pages, whole-page text mockups, long text used as UI, overflow, and invalid containment.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        pageId: { type: 'string', minLength: 1, maxLength: 128 },
        mode: { type: 'string', enum: ['draft', 'handoff'], default: 'handoff' }
      },
      required: ['documentId'],
      additionalProperties: false
    },
    _meta: policy
  }
] as const;
