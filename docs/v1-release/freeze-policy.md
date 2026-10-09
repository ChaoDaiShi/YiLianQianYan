# v1.0.0-rc.1 Freeze Policy

## Allowed after freeze

- release、CI 与验收文档；
- 真人验收证据；
- 明确的 CI 配置修正；
- 已确认的 RC blocker 修复；
- 已确认且影响当前候选的安全漏洞修复。

## Application source fix

任何 Voice、Task、Resource、Artifact、Capability、Memory-to-Skill、Provider、数据库运行逻辑、前端业务逻辑、依赖或打包输入修改，都需要使当前 freeze 失效并重新验证。默认建立 `1.0.0-rc.2`，不得继续沿用 rc.1 文件名和 hash。

## New feature

新功能、顺手优化、架构重构和范围扩展全部 deferred，不进入 rc.1。

## v2

v2、DesktopSpace、App Mount、Wallpaper 和 Windows desktop control 继续 `LOCAL_FROZEN`，不在本 RC 中开发、重验或上传。
