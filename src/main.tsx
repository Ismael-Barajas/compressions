import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
// Self-hosted fonts: no network request at launch, works offline, and not blocked
// by the production CSP (which only allows 'self' for styles and fonts).
import "@fontsource-variable/bricolage-grotesque/opsz.css";
import "@fontsource/ibm-plex-mono/400.css";
import "@fontsource/ibm-plex-mono/500.css";
import "@fontsource/ibm-plex-mono/600.css";
import "./styles/globals.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
