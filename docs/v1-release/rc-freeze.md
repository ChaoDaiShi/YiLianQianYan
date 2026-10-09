# YiLianQianYan v1.0.0-rc.1 RC Freeze

```text
Product:
YiLianQianYan

Candidate:
1.0.0-rc.1

Freeze status:
FROZEN_FOR_HUMAN_ACCEPTANCE

Application source checkpoint:
8394e4495f500f69c57388ebf8510fafcaecbabb

Repository freeze HEAD:
80875f7de76be3b17df596dde9dd9f321d25bc05

Remote branch:
v1/release-work

Foundation:
58db6541dfee599bfa427ef5cafb8b48ad4afc88

Installer:
忆涟千言_1.0.0-rc.1_x64-setup.exe

Installer SHA-256:
6255EFB9D6D7EE572A295974827F3F8133ED71A5C52C912ABC03CBF0BAB7E930

Installer size:
8,740,354 bytes

Technical gate:
PASS according to current-candidate evidence

Remote CI:
BLOCKED_BY_WORKFLOW_SCOPE

Human acceptance:
HUMAN_PENDING

v2:
LOCAL_FROZEN
```

`Repository freeze HEAD` 是封板文档已推送、CI scope 尝试已记录并通过普通 revert 恢复最终 workflow 后的仓库冻结点。此后的交付报告与真人验收证据属于允许的封板后变更，不改变应用源码检查点或安装包身份。

从本记录建立起，v1.0.0-rc.1 不再接受正常功能开发。任何编译或运行时业务源码修改都会使当前 RC freeze 失效。如果必须修改候选应用源码，应重新建立候选版本，优先升级到 `1.0.0-rc.2`，不得用不同二进制静默覆盖现有 rc.1。

正式 v1.0 尚未发布。只有 Remote CI 取得真实 PASS，且 `v1-h-checklist.md` 中适用项全部 ACCEPTED 后，才能按发布流程评估正式版本。
