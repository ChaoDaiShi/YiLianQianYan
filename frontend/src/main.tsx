import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { initializeControlSession } from "./api/controlSession";
import "./index.css";

async function bootstrap() {
  const root = document.getElementById("root") as HTMLElement;
  try {
    await initializeControlSession();
    ReactDOM.createRoot(root).render(
      <React.StrictMode>
        <App />
      </React.StrictMode>,
    );
  } catch (error) {
    console.error("Control session initialization failed");
    root.textContent =
      "安全会话初始化失败。桌面端请重新启动应用；浏览器开发模式请配置 VITE_CONTROL_SESSION_TOKEN。";
    root.setAttribute("role", "alert");
  }
}

void bootstrap();
