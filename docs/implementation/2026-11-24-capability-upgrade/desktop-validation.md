# 桌宠、桌面主题和生态验证

本地证据保存在 `target/capability-upgrade/desktop-validation`。受测 Windows 软件和模型均在本机，截图取自测试内容。

| 编号 | 观测 | 复现入口 |
|---|---|---|
| Live2D | 在 Edge Chromium 中使用官方 Core 文件，经应用的 SHA-256 校验后持久化。真实本地 `miku.model3.json` 的 9 个声明资源导入并在重载后渲染。260 × 280 视口内 WebGL 画布为 244 × 256，采样实际绘制了 16,876 个不透明度大于阈值的像素，页面异常为 0。 | 启动 `npm --prefix frontend run dev -- --host 127.0.0.1 --port 15490`，运行 `node target/capability-upgrade/desktop-validation/live2d-render.mjs`。脚本读取 `YILIAN_TEST_CORE` 指定的官方 Core 文件，或使用临时目录的 `yilian-live2d-official-core.js`。默认模型目录为本机 `F:/Live2D/miku/`，输出 `live2d-local-model.png`。 |
| Windows 图形采集 | 实际 Windows Graphics Capture + D3D11 取得专用合成窗口的一张 582 × 329 PNG，共 21,352 字节。未保存画面文件。测试通过专用窗口句柄构造采集目标，独立验证了项目的帧池、设备、像素读回和 PNG 编码。 | 运行 `target/capability-upgrade/desktop-validation/wgc-smoke.exe`。源码与依赖锁文件在同目录的 `wgc-smoke/`。 |
| Windows 系统选择器 | 独立夹具的 `IInitializeWithWindow` 与 `PickSingleItemAsync` 已运行，实测完成回调没有返回选择目标，进程退出 0。UI Automation 未暴露专用合成窗口的可选择项，未执行确认。 | `target/capability-upgrade/desktop-validation/picker-smoke.exe`、`picker-stdout.txt`。成功授予指定窗口的选择器交互和实际 Tauri 入口仍需联合验证。 |
| Windows 原生壁纸 | `IDesktopWallpaper` 使用 `CLSCTX_LOCAL_SERVER` 成功读取两台显示器，两台均返回当前原生壁纸路径。旧的进程内激活方式在此系统返回 0x80040154。未设置、暂停或替换壁纸。 | 运行 `target/capability-upgrade/desktop-validation/desktop-apps-smoke.exe`，证据在 `desktop-apps-stdout.txt`。 |
| Wallpaper Engine | 本机进程 `wallpaper32` 文件版本 2.8.0.42。独立 CLI 夹具使用合成 MP4 打开专用命名窗口，Win32 枚举确认窗口归属同一个 Wallpaper Engine 进程。按该窗口名关闭后其 HWND 消失，前后两台显示器的 Windows 原生壁纸路径及 Wallpaper Engine 配置文件的 SHA-256 一致。 | `wallpaper-cli-preview.json`。应用设置中的独立预览复用这组限定命令，由原生文件选择器提供 MP4 路径。 [Wallpaper Engine 命令行文档](https://help.wallpaperengine.io/en/functionality/cli.html) |
| Lively Wallpaper | 此次采样未发现 Lively 进程。播放与暂停参数源自官方 CLI 契约。 | 在已开启 Lively 的 Windows 环境打开「更多 → 桌面与桌宠 → 桌面主题」，刷新状态，确认播放或暂停命令并在 Lively 界面读取状态。 |
| VTube Studio | 用可控 WebSocket 验证 API 状态、版本、授权、当前角色、角色加载事件、动画热键、断开连接，并验证更换模型的热键未出现在允许名单中。此次采样没有运行 VTube Studio。 | 与 Live2D 使用同一个 Vite 服务，运行 `node target/capability-upgrade/desktop-validation/vtube-protocol.mjs`。 |
| Rainmeter | 此次采样没有运行 Rainmeter。程序从实际进程发现可执行文件和版本，固定 Manage 入口与以配置名为参数的单皮肤刷新命令。 | 在已开启 Rainmeter 的 Windows 环境刷新「更多 → 桌面与桌宠 → Rainmeter」，打开皮肤管理，并输入一个测试皮肤的配置名确认刷新。 |

## Windows 桌面联调边界

- 当前 Windows Tauri 测试二进制的 `desktop_` 筛选通过 3 项，涵盖采集会话取消和资源释放顺序、壁纸 CLI 参数、单皮肤 Rainmeter 刷新。前端 TypeScript 检查通过。
- 捕获命令检查当前已提交的个人资料。在打开选择器前、得到授权后、返回快照前检查权限。选择器与帧等待共用权限变化检测。独立测试窗口的成功帧覆盖 Windows 图形采集后半段。
- Wallpaper Engine 和 Lively 的整组播放命令按 CLI 退出状态显示。Wallpaper Engine 视频预览按专用窗口的实际存在状态显示。Windows 静态壁纸应用结果按系统 API 的逐显示器回读确认。
- VTube Studio 使用协议模拟端验证交互。官方应用弹窗、授权撤销和真实角色热键是独立的应用联调用例。
- 桌宠的任务提示消费已提交的本地事件。桌面组件显示集中转发的会话状态。测试环境未覆盖全屏应用、Explorer 重启与系统重启的组合。
- 当前 Windows 会话无可用的现成 Sandbox 或 Windows 虚拟机。安装包的实际安装与卸载需要隔离 Windows 环境，当前用户账户未执行安装注册。

## 桌宠任务反馈与发布二进制联调

AT-11 使用无头 Edge、临时工作区和真实本地后端。其余桌面用例通过 Playwright 连接原生 WebView2，资料、工作区和 WebView 数据使用临时目录。从测试应用的设置、聊天和原生命令执行用例，通过 Win32 记录窗口状态。

| 用例 | 结果 | 本地证据 |
|---|---|---|
| AT-11 桌宠反馈 | 通过隔离渲染联调。后端将本地工作流节点执行持久化为 `succeeded`，发出 `v1-task-world` 的 `task.execution.succeeded`。主页面通过生产广播通道转发，桌宠显示「工作流步骤完成」并在提示时限结束后收起。Tauri 外壳 IPC 由夹具替代。 | `pet-task-feedback.json`、`pet-task-feedback.png` |
| AT-12 屏幕权限拒绝 | 通过。聊天入口显示权限提示，IPC 返回 `screen_permission_denied`，页面没有预览。 | `native-desktop-firstpass.json` |
| AT-13 进行中撤权 | 部分通过。原生首轮复现撤权后请求持续等待。修正后将生产协调代码编入合成夹具。待选择和待帧两种情形分别在 210 ms、203 ms 左右自动返回 `screen_permission_revoked`，释放夹具资源，返回 0 字节，随后的请求被拒绝。修正后真实 Windows 选择器自动消失与 GPU 帧进行中的撤权联动未重验。 | `native-desktop-at13-before.json`、`screen-revocation-coordinator.json` |
| AT-14 壁纸软件控制 | 通过。实际桌面主题按钮办理文件选择、取消和预览。Wallpaper Engine 2.8.0.42 创建合成视频的命名窗口并按同名关闭。应用回读的显示器和壁纸路径，以及 Wallpaper Engine 配置 SHA-256 前后一致。 | `native-desktop-firstpass.json` |
| AT-15 外部壁纸软件退出 | 通过受控进程故障注入。临时 `Lively.exe` 由原生入口发现，夹具自行退出后旧暂停动作返回提供者不可用。刷新显示未运行，暂停入口消失。Windows 壁纸路径、正在运行的 Wallpaper Engine 状态和配置 SHA-256 前后一致。 | `native-desktop-at15.json` |
| AT-20 桌面组件崩溃 | 通过。WebView2 返回桌面组件渲染器崩溃事件，Explorer 原 HWND 与 PID 保持可响应。退出隔离应用后旧组件消失，重启应用可重开并关闭组件。 | `native-desktop-firstpass.json` |

无需打开物理桌面选择器即可重跑撤权协调用例。先编译项目的调试依赖，然后运行 `python target/capability-upgrade/desktop-validation/screen-revocation-coordinator.py`。脚本从当前生产源码提取采集协调和会话生命周期，注入合成权限状态与原生等待点，编译并执行 Rust 测试。
