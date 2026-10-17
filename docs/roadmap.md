# 分阶段开发与完成门槛

所有阶段当前均未开始；本轮仅完成仓库骨架及初步评估。

| 阶段 | 交付 | 必须有的证据 |
|---|---|---|
| P0 契约冻结 | 与 ClientKit 对齐的 DTO、核心 endpoint trait、可选能力及来源版本 | Swift/Rust 共用 wire fixtures；错误、缺失/null、序列与能力语义对照 |
| P1 Kernel headless | Kernel adapter、MemoryProjectionStore、Runtime 与回放 CLI | 真实 Kernel connect/list/open/prompt/cancel；五类流、分页固定切点、gap/reconnect、actionable 和重复提交恢复 |
| P2 缓存 | 可切换 MemoryOnly / DurableCache，持久化不阻塞流式展示 | 同一回放最终投影一致；重启、stale、缓存损坏/写失败、身份隔离；性能对照 |
| P3 Pi / DSH | 独立 adapter，各自最小能力集与扩展矩阵 | 捕获回放加真实对应版本连接；unsupported 明确失败；进程退出和恢复 |
| P4 展示状态 | Run、Transcript、Composer、Actionables、健康状态 | 无 UI 的行为用例；草稿隔离、echo 去重、提交回填和 scope 切换 |
| P5 首批 Components | TekesUI RunSurface / TranscriptSurface / Composer | Rust host + 第二 consumer 构建；原生窗口、IME/a11y、长会话滚动与渲染性能 |
| P6 全面对齐 | settings、任务/子代理/SideChat、Summary、AgentThreadList 与工具面 | 逐项设计/接入/体验矩阵、版本 pin、公开 API 示例与发布前检查 |

P1 首先证明本地无客户端磁盘 Mirror 的可行性。P2 验证缓存可插拔。基础闭环完成后再扩大到 Pi/DSH，
避免一次移植所有 ClientKit 扩展。各阶段可缩小首批功能，但必须明示缺失能力，不能伪造统一支持。

评估时不得运行真实 prompt、自动启动 harness 或修改其他仓库；这些属于后续明确的实现/验收阶段。
