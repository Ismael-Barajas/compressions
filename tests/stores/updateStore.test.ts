import { describe, it, expect, beforeEach, vi } from "vitest";

const updater = {
  check: vi.fn(),
};
vi.mock("@tauri-apps/plugin-updater", () => updater);
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn(() => Promise.resolve()) }));

const { useUpdateStore } = await import("../../src/stores/updateStore");

function fakeUpdate(version = "9.9.9") {
  return {
    version,
    body: "notes",
    close: vi.fn(() => Promise.resolve()),
    downloadAndInstall: vi.fn(() => Promise.resolve()),
  };
}

const store = () => useUpdateStore.getState();

beforeEach(() => {
  vi.clearAllMocks();
  useUpdateStore.setState({
    updateAvailable: false,
    updateVersion: null,
    updateNotes: null,
    checking: false,
    downloading: false,
    downloadProgress: 0,
    error: null,
    autoChecked: false,
    toastDismissed: false,
  });
});

describe("updateStore", () => {
  it("hideToast hides the toast but keeps the update available and installable", async () => {
    const update = fakeUpdate();
    updater.check.mockResolvedValue(update);

    await store().checkForUpdate();
    expect(store().updateAvailable).toBe(true);
    expect(store().toastDismissed).toBe(false);

    store().hideToast();
    expect(store().toastDismissed).toBe(true);
    expect(store().updateAvailable).toBe(true);
    expect(store().updateVersion).toBe("9.9.9");

    // pendingUpdate is still held: install goes through.
    await store().installUpdate();
    expect(update.downloadAndInstall).toHaveBeenCalledTimes(1);
    expect(update.close).not.toHaveBeenCalled();
  });

  it("a newly found update re-shows the toast", async () => {
    updater.check.mockResolvedValue(fakeUpdate("1.0.0"));
    await store().checkForUpdate();
    store().hideToast();
    expect(store().toastDismissed).toBe(true);

    updater.check.mockResolvedValue(fakeUpdate("1.1.0"));
    await store().checkForUpdate();
    expect(store().toastDismissed).toBe(false);
    expect(store().updateVersion).toBe("1.1.0");
  });

  it("dismiss forgets the update without clobbering an in-flight check", async () => {
    const update = fakeUpdate();
    updater.check.mockResolvedValue(update);
    await store().checkForUpdate();

    useUpdateStore.setState({ checking: true });
    store().dismiss();

    expect(store().updateAvailable).toBe(false);
    expect(store().checking).toBe(true);
    expect(update.close).toHaveBeenCalledTimes(1);

    // Nothing pending any more.
    await store().installUpdate();
    expect(update.downloadAndInstall).not.toHaveBeenCalled();
  });
});
