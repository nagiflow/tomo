import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";
import { desktop } from "./services/api";
// Native windows are transparent and undecorated; the rounded corners come from CSS.
document.documentElement.classList.toggle("native", desktop);
ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
