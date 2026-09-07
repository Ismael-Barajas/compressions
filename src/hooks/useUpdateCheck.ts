import { useEffect } from "react";
import { useUpdateStore } from "../stores/updateStore";

/**
 * Shared updater state. `autoCheck` runs the startup check once per app session,
 * no matter how many components mount the hook.
 */
export function useUpdateCheck(autoCheck = true) {
  const state = useUpdateStore();

  useEffect(() => {
    if (autoCheck && !useUpdateStore.getState().autoChecked) {
      useUpdateStore.getState().checkForUpdate();
    }
  }, [autoCheck]);

  return state;
}
