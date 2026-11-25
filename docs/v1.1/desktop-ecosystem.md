# 桌宠、屏幕选取与桌面主题

## Live2D 桌宠

从「更多 → 桌面与桌宠 → 桌宠设置」安装 Cubism Core，然后导入含一份 `.model3.json` 的模型文件夹。程序只复制模型声明的资源。可以直接下载与本版锁定的 Core，也可以选择同版本 SDK 的本地 JS 文件。导入时校验 SHA-256，Core 与模型保存在当前桌面 WebView 本地。首次使用时按 Live2D 许可条件取得 SDK。构建时也可在 `frontend/public/desktop-pet/cubism/live2dcubismcore.min.js` 放置通过相同校验的 Core。该文件由打包方按授权提供。 [Live2D Cubism SDK for Web](https://www.live2d.com/en/sdk/download/web/)

角色支持模型自身提供的表情、动作、物理与触摸热区。桌宠设置可把聊天状态映射到模型声明的动作组和表情。只有声明口型参数的模型才响应语音播报的音量信号。桌宠窗口关闭后释放渲染与订阅，默认动画上限为 30 FPS，减少动态效果时为 12 FPS。未载入模型时显示内置 WebP 角色或已配置的头像。

## 屏幕选取

启用个人资料里的「屏幕感知」后，从聊天输入区点击「选取屏幕画面」。Windows 系统选择器只授权本次选中的窗口或显示器，恰好取一张画面。预览前保存在内存，关闭预览即释放。点击「添加到聊天」后按附件流程保存，按「发送」后由当前模型和外部模型权限控制进一步使用。 [Windows 屏幕捕获文档](https://learn.microsoft.com/en-us/windows/uwp/audio-video-camera/screen-capture)

## 壁纸与桌面组件

「桌面主题」可选择当前显示器，并应用本机图片或内置壁纸。应用后读取 Windows 实际壁纸路径。只有主动打开「内置壁纸同步界面明暗」才同步主题。Windows 原生图片设置在第三方壁纸软件退出后可用。实验组件只在用户开启后启动，提供时间与对话状态，可独立关闭。 [Windows IDesktopWallpaper](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-idesktopwallpaper)

Wallpaper Engine 和 Lively Wallpaper 运行时可发送播放或暂停命令，入口会要求确认。外部提供者的进程退出时可刷新可用状态。发送结果按命令进程返回报告，播放状态由外部应用显示。 [Wallpaper Engine CLI](https://help.wallpaperengine.io/en/functionality/cli.html) · [Lively CLI](https://github.com/lively-community/lively/wiki/Command-Line-Controls)

在 Wallpaper Engine 条目点击「在窗口预览视频」并选择本地 MP4，可在独立窗口播放。点击「关闭预览」关闭同名窗口。忆涟千言确认该窗口的实际开关状态后显示结果。 [Wallpaper Engine 命令行窗口预览](https://help.wallpaperengine.io/en/functionality/cli.html#shortcut-example-opening-a-wallpaper-in-a-window)

VTube Studio 通过本机插件 API 建立会话。用户在 VTube Studio 确认授权后，可读取当前角色、监听角色切换并触发动画或表情热键。关闭设置即断开。 [VTube Studio 插件 API](https://github.com/DenchiSoft/VTubeStudio)

Rainmeter 运行时可显示可执行文件版本，打开皮肤管理，并按配置名刷新一个已有皮肤。刷新前显示所影响的配置。 [Rainmeter Bangs](https://docs.rainmeter.net/manual/bangs/)
