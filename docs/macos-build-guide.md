# 忆涟千言 v0.9.0 macOS 构建指南

macOS 构建是独立发布路径，不会由默认 Windows NSIS 命令触发。本指南只描述构建方法；未在 macOS 上执行的签名、公证或运行结果不得标记为通过。

## 前置环境

- macOS 10.15 或更高版本
- Xcode Command Line Tools
- Node.js 18 或更高版本及 npm
- Rust stable toolchain

```bash
xcode-select --install
rustup target add aarch64-apple-darwin x86_64-apple-darwin
```

## 当前架构构建

```bash
chmod +x scripts/build-macos.sh
./scripts/build-macos.sh
```

脚本使用 `npm ci` 安装锁定依赖，并执行：

```bash
npm run tauri:build:macos
```

## Universal 构建

```bash
npm ci
npm --prefix frontend ci
npm run tauri:build:macos:universal
```

## 产物

Cargo workspace 的 bundle 目录为：

```text
target/release/bundle/macos/忆涟千言.app
target/release/bundle/dmg/忆涟千言_0.9.0_<arch>.dmg
```

## 权限

屏幕截图需要“屏幕录制”权限；键盘和鼠标控制需要“辅助功能”权限。首次使用相关能力时，应由用户在“系统设置 → 隐私与安全性”中明确授权。

## 签名与公证

仓库不包含 Apple 凭据。对外分发前需要配置 Developer ID Application 证书，并在独立发布环境完成签名与公证。没有实际验证证据时，发布说明必须标记 macOS 产物为未签名、未公证。
