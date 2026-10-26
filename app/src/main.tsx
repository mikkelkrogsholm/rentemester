import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { createBrowserRouter, RouterProvider } from "react-router-dom";
import { App } from "./App";
import { initializeDocumentStyle } from "./design/document-style";
initializeDocumentStyle();
import { UnsavedChangesProvider } from "./lib/useUnsavedChanges";

const root = document.getElementById("root");
if (!root) throw new Error("missing #root element");

createRoot(root).render(
  <StrictMode>
    <RouterProvider router={createBrowserRouter([{ path: "*", element: <UnsavedChangesProvider><App /></UnsavedChangesProvider> }])} />
  </StrictMode>,
);
