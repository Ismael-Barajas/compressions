import { useEffect } from "react";
import { Header } from "./components/layout/Header";
import { AppShell } from "./components/layout/AppShell";
import { ErrorBoundary } from "./components/layout/ErrorBoundary";
import { UpdateToast } from "./components/update/UpdateToast";
import { useCompressionStore } from "./stores/compressionStore";
import { useUpdateCheck } from "./hooks/useUpdateCheck";
import { getDefaultOutputDir } from "./lib/commands";

function App() {
  const theme = useCompressionStore((s) => s.theme);
  const setOutputDir = useCompressionStore((s) => s.setOutputDir);
  const update = useUpdateCheck();

  useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark");
  }, [theme]);

  // Load default output directory on app start
  useEffect(() => {
    getDefaultOutputDir()
      .then((dir) => {
        const current = useCompressionStore.getState().outputDir;
        if (!current) setOutputDir(dir);
      })
      .catch(() => {
        // Non-fatal — user can still pick a dir manually
      });
  }, [setOutputDir]);

  return (
    <ErrorBoundary>
      <div className="flex h-screen flex-col" style={{ backgroundColor: "var(--bg-primary)" }}>
        <Header />
        <AppShell />
        {update.updateAvailable && update.updateVersion && (
          <UpdateToast
            version={update.updateVersion}
            downloading={update.downloading}
            downloadProgress={update.downloadProgress}
            onInstall={update.installUpdate}
            onDismiss={update.dismiss}
          />
        )}
      </div>
    </ErrorBoundary>
  );
}

export default App;
