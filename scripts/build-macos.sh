#!/bin/bash
# ============================================================
# 忆涟千言 macOS 构建脚本
# YiLianQianYan macOS Build Script
# ============================================================
# Usage:
#   chmod +x scripts/build-macos.sh
#   ./scripts/build-macos.sh
# ============================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo "=============================================="
echo "  忆涟千言 macOS 构建"
echo "  YiLianQianYan macOS Build"
echo "=============================================="
echo ""

# Check prerequisites
echo "[1/5] 检查构建环境..."

# Check Node.js
if ! command -v node &> /dev/null; then
    echo "❌ Node.js 未安装，请先安装 Node.js 18+"
    echo "   下载地址: https://nodejs.org/"
    exit 1
fi
echo "   ✓ Node.js $(node --version)"

# Check npm
if ! command -v npm &> /dev/null; then
    echo "❌ npm 未安装"
    exit 1
fi
echo "   ✓ npm $(npm --version)"

# Check Rust
if ! command -v rustc &> /dev/null; then
    echo "❌ Rust 未安装，请先安装 Rust"
    echo "   安装命令: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi
echo "   ✓ Rust $(rustc --version)"

# Check Xcode command line tools
if ! xcode-select -p &> /dev/null; then
    echo "❌ Xcode Command Line Tools 未安装"
    echo "   安装命令: xcode-select --install"
    exit 1
fi
echo "   ✓ Xcode Command Line Tools"

echo ""
echo "[2/5] 安装前端依赖..."
cd "$PROJECT_ROOT/frontend"
npm ci
echo "   ✓ 前端依赖安装完成"

echo ""
echo "[3/5] 安装 Tauri CLI..."
cd "$PROJECT_ROOT"
npm ci
echo "   ✓ Tauri CLI 安装完成"

echo ""
echo "[4/5] 构建 macOS 安装包..."
echo "   目标: .app 和 .dmg"
npm run tauri:build:macos
echo "   ✓ 构建完成"

echo ""
echo "[5/5] 查找构建产物..."
BUILD_DIR="$PROJECT_ROOT/target/release/bundle"
echo ""
echo "=============================================="
echo "  构建完成！"
echo "=============================================="
echo ""
echo "构建产物位置:"
echo "  App 包:  $BUILD_DIR/macos/忆涟千言.app"
echo "  DMG 安装包: $BUILD_DIR/dmg/忆涟千言_1.0.0-rc.1_<arch>.dmg"
echo ""

if [ -d "$BUILD_DIR" ]; then
    echo "找到的构建文件:"
    find "$BUILD_DIR" -type f \( -name "*.dmg" -o -name "*.app" \) 2>/dev/null | head -20
fi

echo ""
echo "提示:"
echo "  - .app 是应用程序包，可以直接拖到 应用程序 文件夹安装"
echo "  - .dmg 是磁盘镜像安装包，用于分发"
echo "  - 如需 Apple 签名和公证，请配置签名证书后重新构建"
echo ""
