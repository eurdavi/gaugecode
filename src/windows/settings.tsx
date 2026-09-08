import React from "react";
import ReactDOM from "react-dom/client";
import { Settings } from "../components/Settings";
import "../styles.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Settings />
  </React.StrictMode>,
);
