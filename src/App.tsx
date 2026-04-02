import { useEffect } from "react";
import { Header } from "./components/layout/Header";
import { AppShell } from "./components/layout/AppShell";
import { useCompressionStore } from "./stores/compressionStore";

function App() {
  const theme = useCompressionStore((s) => s.theme);

  useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark");
  }, [theme]);

  return (
    <div className="flex h-screen flex-col" style={{ backgroundColor: "var(--bg-primary)" }}>
      <Header />
      <AppShell />
    </div>
  );
}

export default App;
