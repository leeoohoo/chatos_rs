export type PetSurfaceRequest = "quick-chat" | "notepad";

export interface PetSurfaceRequestPort {
  subscribe(listener: (request: PetSurfaceRequest) => void): Promise<() => void>;
}
