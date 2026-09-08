import React from "react";
import ReactDOM from "react-dom/client";
import { NotchShell } from "../components/NotchShell";
import "../styles.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <NotchShell />
  </React.StrictMode>,
);
