import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "@photo-cake/ui";
import "@photo-cake/ui/styles.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App mode="companion" />
  </StrictMode>,
);
