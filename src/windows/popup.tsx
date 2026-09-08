import React from "react";
import ReactDOM from "react-dom/client";
import { TrayPopup } from "../components/TrayPopup";
import "../styles.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <TrayPopup />
  </React.StrictMode>,
);
