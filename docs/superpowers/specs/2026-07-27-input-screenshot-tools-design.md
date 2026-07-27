# 鼠标/键盘模拟 & 屏幕截图工具 — 设计文档

**日期**：2026-07-27  
**状态**：已批准  
**目标**：为沙箱页面查看能力提供底层输入模拟和屏幕感知工具

---

## 1. 背景

忆涟千言当前拥有文件操作、搜索、HTTP、系统命令执行等工具，但缺少图形界面交互能力。要让 AI 智能体实现"沙箱页面查看"——看见屏幕内容并操作 GUI——需要两类新工具：

- **输入模拟**：鼠标和键盘操作，控制桌面 GUI
- **屏幕感知**：截图捕获屏幕内容，作为 LLM 视觉输入

---

## 2. 技术决策

采用 **Rust 原生跨平台 crate** 方案（方案 A）：

| 功能 | Crate | Windows | macOS | Linux |
|------|-------|---------|-------|-------|
| 鼠标/键盘模拟 | `enigo` 0.3 | SendInput API | CGEvent API | xdo/evdev |
| 屏幕截图 | `xcap` 0.3 | DXGI | CGImage | X11 SHM |

选用理由：零外部系统依赖、编译后开箱即用、截图性能好（直接返回像素缓冲）、类型安全。

---

## 3. 文件变更

```
backend/Cargo.toml          +3 行（enigo, xcap, base64 依赖）
backend/src/tools/input.rs  +1 新文件（鼠标 + 键盘工具）
backend/src/tools/screenshot.rs  +1 新文件（截图工具）
backend/src/tools/mod.rs    +2 行（pub mod 声明）
backend/src/tools/registry.rs  +2 行（注册新工具）
```

---

## 4. 工具 API 设计

### 4.1 `screenshot` — 截图

**路由**：`backend/src/tools/screenshot.rs`
**启用 `requires_approval`**：是（涉及视觉隐私）

```jsonc
// 输入参数
{
  "monitor": "number   // 显示器索引，默认0(主显示器)",
  "x":       "number   // 区域左上角X，不传则全屏",
  "y":       "number   // 区域左上角Y",
  "width":   "number   // 区域宽度",
  "height":  "number   // 区域高度",
  "format":  "string   // png | jpg，默认png"
}

// 返回（仅成功时附带元数据）
{
  "content": "iVBORw0KGgo...(base64)",
  "ok": true,
  "metadata": {
    "width": 1920,
    "height": 1080,
    "format": "png",
    "monitor": 0
  }
}
```

**执行流程**：

```
xcap::Monitor::all()
  → 按 monitor 索引取目标
  → monitor.capture_image() → DynamicImage
  → (可选) imageops::crop(x, y, w, h) 区域截取
  → image crate 编码为 PNG/JPEG → Vec<u8>
  → base64::Engine::encode() → String
  → serde_json::json!({ content, metadata })
```

### 4.2 `mouse` — 鼠标控制

**路由**：`backend/src/tools/input.rs`（与 keyboard 共享文件）

| Action | 含义 | 必须参数 | 可选参数 |
|--------|------|---------|---------|
| `move` | 移动到绝对坐标 | x, y | — |
| `click` | 单点 | x, y | button (默认 left) |
| `double_click` | 双击 | x, y | button (默认 left) |
| `drag` | 拖拽 | from_x, from_y, to_x, to_y | button (默认 left) |
| `scroll` | 滚轮 | direction (up/down/left/right) | amount (默认3) |

```jsonc
// 参数
{
  "action":    "string   // move | click | double_click | drag | scroll",
  "x":         "number   // 目标X（move/click/double_click）",
  "y":         "number   // 目标Y",
  "from_x":    "number   // 拖拽起始X（drag）",
  "from_y":    "number   // 拖拽起始Y（drag）",
  "to_x":      "number   // 拖拽目标X（drag）",
  "to_y":      "number   // 拖拽目标Y（drag）",
  "button":    "string   // left | right | middle，默认left",
  "direction": "string   // up | down | left | right（scroll）",
  "amount":    "number   // 滚动行数，默认3"
}

// 返回（成功示例）
"鼠标已移动到 (800, 600)"
"已在 (400, 300) 执行 left 单击"
"滚轮向上滚动 5 行"
```

### 4.3 `keyboard` — 键盘控制

**路由**：`backend/src/tools/input.rs`

| Action | 含义 | 必须参数 | 说明 |
|--------|------|---------|------|
| `type` | 输入文本 | text | 支持中英文，逐字符输入 |
| `press` | 单键 | key | 按一下就释放 |
| `combo` | 组合键 | keys | 如 ["ctrl", "c"]，按序按下→逆序释放 |
| `key_down` | 按住 | key | 不释放（配合 drag 等高级场景） |
| `key_up` | 释放 | key | 释放此前按住的键 |

```jsonc
// 参数
{
  "action": "string   // type | press | combo | key_down | key_up",
  "text":   "string   // 文本内容（action=type）",
  "key":    "string   // 按键名（action=press/key_down/key_up）",
  "keys":   "string[] // 组合键数组（action=combo），如 [\"ctrl\", \"shift\", \"t\"]"
}

// 返回（成功示例）
"已输入文本：你好 world"
"已按下组合键: ctrl+c"
```

### 4.4 按键名映射表

用 `HashMap<&'static str, enigo::Key>` 做常量表：

```
功能键：return, tab, space, escape, backspace, delete,
        insert, home, end, page_up, page_down, print_screen, pause
方向键：up, down, left, right
修饰键：shift, control, alt, meta, caps_lock, num_lock, scroll_lock
F键：  f1, f2, ... f12
字母：  a-z (小写)
数字：  0-9
符号：  ``, -, =, [, ], \, ;, ', ,, ., / 等
```

不在表中的键名返回 `ToolResult::error("未知按键: {name}")`。

---

## 5. 错误处理

所有工具统一错误格式，沿用既有 `ToolResult`：

```rust
// 场景覆盖
// screenshot
ToolResult::error("截图失败：无可用显示器")
ToolResult::error("区域超出屏幕范围 (1920x1080)")

// mouse
ToolResult::error("坐标超出屏幕范围")
ToolResult::error("无效的鼠标按钮: foo，支持 left/right/middle")

// keyboard
ToolResult::error("未知按键: foo")
ToolResult::error("type 操作需要 text 参数")
ToolResult::error("combo 操作需要 keys 数组")
```

enigo 初始化失败（极少见，通常是因为图形会话不可用）时统一返回：
```
ToolResult::error("输入系统不可用：当前环境不支持输入模拟")
```

---

## 6. 沙箱 & 安全考量

| 考量 | 策略 |
|------|------|
| 屏幕隐私 | `screenshot` 启用 `requires_approval`，用户需明确允许 |
| 输入安全 | `mouse`/`keyboard` 不启用审批，与 `bash` 对齐（bash 能执行 PowerShell 全控桌面） |
| 内存保护 | 截图数据不写文件，仅通过 base64 字符串在内存中传递给 LLM |
| 跨平台 | macOS/Linux 下 `enigo` 可能需要辅助功能权限，由用户操作系统层授权 |

---

## 7. 依赖版本

```toml
# backend/Cargo.toml 新增
enigo = { version = "0.3", features = ["serde"] }
xcap = "0.3"
base64 = "0.22"
```

---

## 8. 测试策略

- **手动验证**（自动化测试受限，这些工具依赖真实桌面环境）：
  - Windows 上启动后端，通过 `/api/chat` 发送工具调用验证三类工具
  - 截图工具：全屏截图 → base64 解码验证为有效 PNG
  - 鼠标工具：移动→截图对比、点击记事本、双击选中文字
  - 键盘工具：打开记事本 → type 输入 → combo Ctrl+A → press delete
- 按键映射表可添加单元测试验证字符串→枚举映射覆盖率
