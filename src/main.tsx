import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import { createBackend } from "./lib/backend";
import { createTelemetryStore } from "./lib/telemetry";
import { initializeTheme } from "./lib/theme";
import "./styles.css";

initializeTheme();
const root = ReactDOM.createRoot(document.getElementById("root")!);
createBackend()
  .then((backend) => {
    root.render(
      <React.StrictMode>
        <App backend={backend} store={createTelemetryStore(backend)} />
      </React.StrictMode>,
    );
  })
  .catch((error) => {
    root.render(
      <main className="startup-screen">
        <h1>OpenMemDuct</h1>
        <p role="alert">{String(error)}</p>
      </main>,
    );
  });
