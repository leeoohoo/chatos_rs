from __future__ import annotations

import warnings
from pathlib import Path

from gateway_core.sdk_loader import load_sdk_imports


REPO_ROOT = Path(__file__).resolve().parents[2]
GATEWAY_ROOT = Path(__file__).resolve().parents[1]

warnings.filterwarnings(
    "ignore",
    message=r'Field "model_.*" has conflict with protected namespace "model_".*',
    category=UserWarning,
)

SDK_IMPORT_SOURCE, _sdk_imports = load_sdk_imports(
    repo_root=REPO_ROOT,
    gateway_root=GATEWAY_ROOT,
)


(
    AppServerClient,
    AppServerConfig,
    AgentMessageDeltaNotification,
    AgentMessageThreadItem,
    CommandExecutionThreadItem,
    DynamicToolCallThreadItem,
    FileChangeThreadItem,
    ImageViewThreadItem,
    ItemCompletedNotification,
    ItemStartedNotification,
    McpToolCallThreadItem,
    ModelListResponse,
    ReasoningSummaryTextDeltaNotification,
    ReasoningTextDeltaNotification,
    ReasoningThreadItem,
    ThreadTokenUsageUpdatedNotification,
    TurnCompletedNotification,
    WebSearchThreadItem,
) = _sdk_imports
