# #23：同 scope Task source 更换的内容退休

关联 #23、#15、[PR #10](https://github.com/TekesApps/TekesAgentKit/pull/10)。[设计](rust-agent-ui-task-content-retirement-contract-20261025.md) · [机器收据](rust-agent-ui-task-content-retirement-results-20261025.json)。基线 bd5e7f5；ClientKit 只读参考 c289bc9、Tekes 8858ccd，SDK 精确公开 pin 50ec49f。

新回归先失败于旧 Composer root 仍存在；旧 Run access 已 Missing，说明权限退休与 UI 退休是两项不同要求。修复 SidePanel 的显式目标内容退休 API，并在 demo 识别新 Task source owner 时调用。旧 token 失效，其他 tab token、header 与普通 facts 更新的控件实例保留。私有实例 marker 也拒绝同 namespace 重建面板的旧 token。host 在卸载前清焦点，仍先执行既有 composition gate；不让 UI token 替代 async binding admission。

实际验证：

- UI workspace 790 通过、0 失败、1 项既有忽略。
- 最终源码默认/feature 各 15 项定向测试通过：active/inactive 内容退休、其他 tab 保留、stale/fresh/foreign token、同 namespace 新实例、无 custom 内容、普通 facts 实例保留；同 parent/task/scope 不同 source，原 OpenTask 命令 owner/目标正确，新 Run ready 前后旧 Run 迟到均拒绝，新 Composer 和父非空中文草稿独立，关闭撤销新 Run。
- 默认 workspace 与 feature demo 全 targets 严格 Clippy、格式、diff、Rust UI 边界检查通过；consumer Python 3 项通过。
- 独立 consumer 只有公开组件依赖，实际编译执行新 retire API，旧 token 重复调用被拒绝、fresh token 可再次退休。五份验收可执行文件已归档；592 个源码/锁/固定输入均冻结，提交后另核 Git 对应。

本切片是内存 SEP 与真实公开业务组件/消费宿主的生命周期回归；不证明权威真实 Task producer、桌面窗口、物理 IME、OS AX、ProcessContainer 或全部 View parity。790 项全套之后仅补充 token 解释注释和 foreign-install 测试，最终默认/feature 定向测试与 Clippy 又从冻结源码运行。#23 保持开放，#42/#53 门槛不豁免。

前一 bd5e7f5 两轮 CI 40142666301 / 40142670357 已终态核验：headless、默认 UI、opt-in、独立 consumer 通过；唯一失败仍是 TekesUI #53 严格 nested reading，整体 failure。本提交 CI 单独跟踪。无上游代码改动、桌面/截图/TCC操作、新 target 或缓存删除；共享 SSD target 和历史证据保留。本轮实际删除缓存 0 字节。
