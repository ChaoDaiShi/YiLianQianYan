@echo off
title 忆涟千言 Desktop Dev
cd /d "%~dp0"
echo ========================================================
echo 正在启动 忆涟千言 桌面端 (Tauri v2 + React 18 + Vite)...
echo ========================================================
npm run tauri:dev
pause
