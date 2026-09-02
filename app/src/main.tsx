import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./ui/App";

const root = document.getElementById("root");

if (!root) {
  throw new Error("LedgerKit root element is missing");
}

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
