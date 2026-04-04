import { useEffect } from "react";
import { Header } from "./components/layout/Header";
import { AppShell } from "./components/layout/AppShell";
import { useCompressionStore } from "./stores/compressionStore";
import { getDefaultOutputDir } from "./lib/commands";

function App() {
  const theme = useCompressionStore((s) => s.theme);
  const setOutputDir = useCompressionStore((s) => s.setOutputDir);

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
    <div className="flex h-screen flex-col" style={{ backgroundColor: "var(--bg-primary)" }}>
      <Header />
      <AppShell />
    </div>
  );
}

export default App;
