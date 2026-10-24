# Akasha-WeChat 渠道接入

本文记录 **wechat_onebot** provider 的实际协议边界、Akasha 本地补丁和 WeFlow 前置条件。该渠道使用 OneBot 11，不是 OneBot 12，也不调用微信 iLink 登录流程。

## 能力与数据语义

Akasha-WeChat 把 WeFlow 的实时消息转换成 OneBot 11 消息事件，再通过反向 WebSocket 发给接收端。Rust 渠道把 OneBot 11 的 numeric self_id、user_id、group_id 校验后接入。私聊的 peer_id 是 user_id；群聊的 peer_id 是 group_id，sender_id 是发言成员。

Akasha 原版事件没有 message_id，也不会转发 WeFlow SSE 中的 rawid。配套补丁为每个逻辑事件增加唯一 bridge_event_id，本应用据此去重；external_message_id 保持空值。两条同秒同正文消息有不同的桥接事件 ID，不会被合并。此 ID 是桥接身份，不是微信消息 ID；未应用配套补丁的事件会明确拒绝。

Akasha OneBot handler 接受 send_msg、send_private_msg 和 send_group_msg。本项目发私聊和群聊时分别使用 send_private_msg / send_group_msg，并只发送文本段。Akasha 不提供联系人、群列表、最近会话或历史查询 OneBot action；此 provider 不把 WeFlow 另有的 HTTP API 能力冒充为 Akasha 能力，也不在本轮接入历史读取。

上游 README 提到 weflow_api 发送方式，但固定提交的 senders.py 工厂只创建 UiaSender。此补丁只支持文本和 UIA，不宣称 WeFlow 发送 API、图片、语音或其他媒体发送可用。

## 发送回执

未打补丁的 Akasha 会在调用 UIA 发送前回传 status=ok, retcode=0, data={}。这只表示 bridge 收到 action，不能表示文字已经进入微信窗口或已送达。本项目将空 data 归类为“桥接器已接收，送达未知”，不会重发。

随仓库提供的显式补丁把成功回执移动到 UiaSender.send_text 返回之后，数据为 {"ui_action_completed":true}。这只代表上游 UIA 函数返回成功；Akasha 没有微信服务端 receipt 或真实 message_id，也没有手机端收件确认。运行记录不能把它显示成已验证送达。UIA 仍通过联系人搜索并选取首个结果，接收对象也未由微信界面结构化校验，因此仅对明确授权且 bridge 已识别的目标启用自动发送。

如果 bridge 在发出 action 后、回执发出前断开，送达状态未知；客户端不自动重试该写操作。

## 认证和 ID

接入需要 X-Self-ID 与 Authorization: Bearer <onebot_access_token>。补丁给 Akasha 增加独立 onebot_access_token 配置项。它必须与渠道保存的 OneBot access token 一致，不复用 WeFlow access_token，也不写进 WebSocket URL 或日志。

Akasha 原来的 ID 使用 Python hash()，默认受 PYTHONHASHSEED 影响，重启后可能变化。补丁改成 SHA-256 派生的非负 63-bit OneBot ID，并在启动日志输出 self_id，便于配置渠道 self_id。私聊对象使用 WeFlow sessionId 派生 ID；群 peer 使用群 sessionId；群成员 ID 仍由群 session 加成员展示名派生，属于 bridge ID，不是成员原始 wxid。成员改名后其 ID 可能改变。

bridge 通过进程内的 ID 到联系人展示名映射发送。该映射重启后重建，只有 bridge 本次运行已收到的联系人或群事件会加入映射。补丁在目标 ID 没有映射时拒绝发送，避免把数字 ID 当作联系人名称交给 UIA 搜索。

## 上游补丁

补丁文件：[akasha-weflow-onebot11.patch](patches/akasha-weflow-onebot11.patch)

补丁针对 alingalingling/Akasha-WeChat 的 wechat-weflow-bridge-ob11，固定上游 main 提交 9dbe933a9f1bc46e33ca8121334bb1dd01173248。它只改 bridge 配置、OneBot WebSocket 客户端、ID 派生和 action 回执，不移植或改写 Python 后端。

补丁也避免把带 access_token 查询参数的 SSE URL 写进日志；WeFlow token 仍由 bridge 用于本机 API 请求。

在 Akasha 仓库根目录检查并应用：

    git apply --check <path-to-akasha-weflow-onebot11.patch>
    git apply <path-to-akasha-weflow-onebot11.patch>

如果上游文件与固定提交不同，先审阅补丁上下文再决定是否移植；不要跳过 --check。

## WeFlow 与本机准备

Akasha README 要求 Windows 桌面环境、已登录的桌面微信和 WeFlow。WeFlow 需要先由使用者完成本机微信数据连接，然后在设置中开启 HTTP API 服务和主动推送。官方 HTTP API 文档默认服务地址为 http://127.0.0.1:5031；除健康检查外接口需要 API token。Akasha 使用该地址检查 /api/v1/messages，并从 /api/v1/push/messages?access_token=... 订阅 SSE。

配置 Akasha bridge：

1. 在 WeFlow 的设置页启用 HTTP API 和主动推送，复制 WeFlow API token。
2. 在 Akasha 配置中分别填写 weflow_base_url 与 WeFlow access_token。
3. 填写 bridge 的微信 bot_wxid 和群 @ 检测用的 bot_nicknames。
4. 填入 onebot_access_token，值与本应用渠道中的 OneBot token 完全相同。
5. 把 astrbot_ob_url 指向本应用渠道页面生成的 reverse-WebSocket 地址，并将启动日志中的 self_id 填入渠道。
6. 保存后启动 bridge；连接与真实消息收发仍需在使用者自选的测试对象上进行实际验收。

WeFlow 相关状态按 2026-10-24 检查记录如下：

- 本机没有发现 WeFlow / WeChat 进程，也没有 TCP 5031 监听；没有读取 WeChat 数据库、WeFlow 配置或任何 token。
- 官方 WeFlow 仓库 README 说明 Windows 10+ x64 提供 .exe 安装包，官网有下载区域；但官方 GitHub Releases 页面当前显示没有发布。官网页面的下载区域可能通过客户端脚本动态加载，本轮没有下载或安装，因此未确认当前可取得的安装包版本。
- 上游 issue [#1222](https://github.com/hicccc77/WeFlow/issues/1222) 记录 WeFlow 5.0.0 Windows 原生组件在 2026-09-30 后报告 -101 / -1000 数据库初始化错误；这不是已验证的正式修复通知。应在安装前核实官方来源、具体版本和 API 实际状态，不要把第三方 community build 当成官方包。
- 本渠道的 Rust 解析、动作构造和回执分类不依赖本机安装；用户已明确选择本轮先完成接入代码，WeFlow 环境稍后补齐；真实账号消息验收标记为 HUMAN_PENDING。

## 来源

- [Akasha-WeChat README](https://github.com/alingalingling/Akasha-WeChat/blob/9dbe933a9f1bc46e33ca8121334bb1dd01173248/README.md)
- [Akasha OneBot protocol](https://github.com/alingalingling/Akasha-WeChat/blob/9dbe933a9f1bc46e33ca8121334bb1dd01173248/wechat-weflow-bridge-ob11/ob_protocol.py)
- [Akasha OneBot event builder](https://github.com/alingalingling/Akasha-WeChat/blob/9dbe933a9f1bc46e33ca8121334bb1dd01173248/wechat-weflow-bridge-ob11/bridge_core.py)
- [Akasha WebSocket client](https://github.com/alingalingling/Akasha-WeChat/blob/9dbe933a9f1bc46e33ca8121334bb1dd01173248/wechat-weflow-bridge-ob11/ob_client.py)
- [WeFlow HTTP API / Push 文档](https://github.com/hicccc77/WeFlow/blob/main/docs/HTTP-API.md)
- [WeFlow official releases](https://github.com/hicccc77/WeFlow/releases)

配套补丁还为每个事件增加 bridge_event_id；UIA 开始后发生失败或异常会返回已尝试但结果未知，接收端保留 delivery_unknown，不把此情况当作明确未发送。
