# 分阶段开发与完成门槛

P0/P1/P3 已完成本轮三个 SEP 的非 UI 会话闭环，验收见 session-endpoints.md 和 headless-kernel.md；不代表完整 ClientKit API 或 UI projector 对齐。缓存和 UI 尚未开始；P4 的 Run/Transcript/Composer/连接状态已在无 UI presentation 层实现，边界见 headless-presentation.md。Tasks/Subagents/SideChat/Summary 的无 UI 状态已补充，原生能力及尚需宿主输入的部分见 headless-agent-panels.md。

| 阶段 | 交付 | 必须有的证据 |
|---|---|---|
| P0 契约冻结 | 与 ClientKit 对齐的 DTO、核心 endpoint trait、可选能力及来源版本 | 语言无关 wire fixtures 与 Rust 回归；错误、缺失/null、序列与能力语义对照 |
| P1 Kernel headless | Kernel adapter、MemoryProjectionStore、Runtime 与回放 CLI | 真实 Kernel connect/list/open/prompt/cancel；当前 v4 的 inventory/control/actionables + journal、分页固定切点、gap/reconnect、actionable 和重复提交恢复 |
| P2 缓存 | 可切换 MemoryOnly / DurableCache，持久化不阻塞流式展示 | 同一回放最终投影一致；重启、stale、缓存损坏/写失败、身份隔离；性能对照 |
| P3 Pi / DSH | 独立 adapter，完整会话生命周期与显式扩展矩阵 | 捕获回放加真实对应版本连接；unsupported 明确失败；进程退出和恢复 |
| P4 展示状态 | Run、Transcript、Composer、Actionables、健康状态 | 无 UI 的行为用例；草稿隔离、echo 去重、提交回填和 scope 切换 |
| P5 首批 Components | TekesUI RunSurface / TranscriptSurface / Composer | Rust host + 第二 consumer 构建；原生窗口、IME/a11y、长会话滚动与渲染性能 |
| P6 全面对齐 | settings、任务/子代理/SideChat、Summary、AgentThreadList 与工具面 | 逐项设计/接入/体验矩阵、版本 pin、公开 API 示例与发布前检查 |

P1 首先证明本地无客户端磁盘 Mirror 的可行性。P2 验证缓存可插拔。三个独立 Endpoint 的非 UI 会话能力已接入；产品专属服务由显式 HostServices 提供，UI projector 和控件另行推进。各阶段可缩小首批功能，但必须明示缺失能力，不能伪造统一支持。

2026-10-17 用户已授权 Kernel + 内存投影无 UI 实现与验证。允许隔离真实 Kernel + 本地合成 provider 验收；不修改其他仓库、不接 UI。

2026-10-17 后续授权：接入 Native / Pi / DSH 三个独立 SessionEndpoint，完成无 UI 的回传、历史、定位和文件读取。当前按公开协议对接，真实运行时使用隔离状态和合成 provider 验证。

DSH 已升级并隔离验证 0.2.1-rc.2（保留 0.2.0-rc.2），补齐普通 fork、归档恢复、冷客户端冻结分页、新工作目录和 job 事件兼容。详见 session-endpoints.md 的升级验收。

2026-10-18：补齐 P4 非 UI presentation 和独立公开 consumer；不包含 P5 控件与完整 P6 产品面。

2026-10-18：v0.1.0 保存 headless 阶段；v0.1.1 增加 public/MIT 与公开接入、CLI、贡献和版本文档，crate 继续 publish=false。
