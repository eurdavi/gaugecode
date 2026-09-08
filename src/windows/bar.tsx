import React from "react";
import ReactDOM from "react-dom/client";
import { TaskbarBar } from "../components/TaskbarBar";
import "../styles.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <TaskbarBar />
  </React.StrictMode>,
);
