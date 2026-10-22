import { z } from "zod";

export const deviceGatewaySnapshotSchema = z.object({
  configured: z.boolean(),
  connectorRunning: z.boolean(),
  deviceName: z.string().min(1),
  deviceId: z.string().min(1),
  cloudBaseUrl: z.string().url(),
  workspaceCount: z.number().int().nonnegative(),
  runtime: z.string().min(1),
  developerMode: z.boolean(),
  connectionMode: z.literal("authenticated_control_plane"),
  deviceSignedRelayAvailable: z.boolean(),
});

export type DeviceGatewaySnapshot = z.infer<typeof deviceGatewaySnapshotSchema>;
