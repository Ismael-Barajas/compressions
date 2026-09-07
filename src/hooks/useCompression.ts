import { compressionController, type CompressionController } from "../lib/compressionController";

/**
 * Access to the compression driver. The controller is a module-level singleton with
 * stable function identities, so this hook creates no subscriptions and no
 * per-instance callbacks — safe to call from every list row.
 */
export function useCompression(): CompressionController {
  return compressionController;
}
