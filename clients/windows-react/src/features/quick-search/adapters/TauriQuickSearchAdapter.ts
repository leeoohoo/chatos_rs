import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { quickSearchBootstrapSchema, quickSearchSnapshotSchema } from "../domain/quickSearch";
import type {
  QuickSearchNavigationPort,
  QuickSearchNavigationRequest,
  QuickSearchPort,
} from "../ports/QuickSearchPort";

export class TauriQuickSearchAdapter implements QuickSearchPort, QuickSearchNavigationPort {
  async bootstrap() {
    return quickSearchBootstrapSchema.parse(await invoke("load_quick_search"));
  }

  async search(
    presentationId: string,
    query: string,
    requestId: number,
    includeFiles: boolean,
  ) {
    return quickSearchSnapshotSchema.parse(await invoke("query_quick_search", {
      input: { presentationId, query, requestId, includeFiles },
    }));
  }

  async execute(presentationId: string, resultId: string) {
    await invoke("execute_quick_search", { input: { presentationId, resultId } });
  }

  async close() {
    await invoke("close_quick_search");
  }

  async subscribePresented(listener: () => void) {
    return listen("quick-search-presented", listener);
  }

  async subscribeNavigation(listener: (request: QuickSearchNavigationRequest) => void) {
    return listen<QuickSearchNavigationRequest>("quick-search-navigation-requested", ({ payload }) => {
      listener(payload);
    });
  }
}
