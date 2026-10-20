import type {
  PluginApplication,
  PluginApplicationContextSelection,
  PluginApplicationLaunch,
} from "../domain/pluginApplications";

export interface PluginApplicationLaunchRequest {
  ownerUserId: string;
  presentationId: string;
  generation: number;
  pluginId: string;
  componentKey: string;
  context: PluginApplicationContextSelection;
}

export interface PluginApplicationPresentationScope {
  ownerUserId: string;
  presentationId: string;
  generation: number;
}

export interface PluginApplicationBridgeRequest extends PluginApplicationPresentationScope {
  sessionId: string;
  hostSessionNonce: string;
  origin: string;
  requestId: string;
  method: string;
  payload: Record<string, unknown>;
}

export interface PluginApplicationBridgeResponse {
  requestId: string;
  result: unknown;
}

export interface PluginApplicationPort {
  list(ownerUserId: string): Promise<PluginApplication[]>;
  launch(request: PluginApplicationLaunchRequest): Promise<PluginApplicationLaunch>;
  discardPresentation(request: PluginApplicationPresentationScope): Promise<void>;
  handleBridgeRequest(
    request: PluginApplicationBridgeRequest,
  ): Promise<PluginApplicationBridgeResponse>;
}
