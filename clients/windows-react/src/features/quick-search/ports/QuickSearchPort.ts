import type { QuickSearchBootstrap, QuickSearchSnapshot } from "../domain/quickSearch";

export interface QuickSearchPort {
  bootstrap(): Promise<QuickSearchBootstrap>;
  search(
    presentationId: string,
    query: string,
    requestId: number,
    includeFiles: boolean,
  ): Promise<QuickSearchSnapshot>;
  execute(presentationId: string, resultId: string): Promise<void>;
  close(): Promise<void>;
  subscribePresented(listener: () => void): Promise<() => void>;
}

export interface QuickSearchNavigationRequest {
  ownerUserId: string;
  destination: "project" | "contact" | "settings" | "runtimePermissions";
  resourceId?: string | null;
}

export interface QuickSearchNavigationPort {
  subscribeNavigation(listener: (request: QuickSearchNavigationRequest) => void): Promise<() => void>;
}
