import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import ConfigErrorScreen from "./ConfigErrorScreen";
import { EnvConfigError, readEnvConfig } from "./lib/envConfig";
import "./styles.css";

const root = createRoot(document.getElementById("root")!);
const envResult = readEnvConfig(import.meta.env);

if (!envResult.ok) {
  if (import.meta.env.DEV) {
    // Development fails loudly: Vite's error overlay shows this at boot
    // instead of rendering an app that cannot talk to any contract.
    throw new EnvConfigError(envResult.issues);
  }
  // Production renders a configuration-error screen rather than letting
  // every contract call fail individually.
  root.render(
    <StrictMode>
      <ConfigErrorScreen issues={envResult.issues} />
    </StrictMode>,
  );
} else {
  root.render(
    <StrictMode>
      <App initialConfig={envResult.config} />
    </StrictMode>,
  );
}
