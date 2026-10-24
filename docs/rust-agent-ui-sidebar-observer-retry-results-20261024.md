# Sidebar observer Retry 用户入口交付（#22）

实现 `be4fc58b54fd65b4408e8f150eac842322948e40`，PR [#10](https://github.com/TekesApps/TekesAgentKit/pull/10)。沿用[设计契约](rust-agent-ui-sidebar-observer-retry-ui.md)，只消费精确 TekesUI `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`；没有修改 SDK、Kernel、Swift 或启动桌面。

## 实现

- 单端和固定多端 binding 的 `observer_snapshot` 将同一不可变原 publication 与 opaque 来源键配对。多端使用该次原 child status，不与较新 child snapshot 拼接。可编辑 publication、公开可克隆 source key 都不能派生新代的真实授权。
- 状态类型放入 presentation，bindings 保持 re-export。Failed 才提供 Retry；Restarting 保留诊断，健康且已收敛状态不占 footer。原 key 交回原 binding，经过 capture 与 restart 两次校验；不替换当前代数、不重连 transport、不重发 mutation。
- `SidebarObserverStatus` 只组合默认 Text/Button/Stack；Sidebar 将其放在独立 footer，保留 search footer。单端/多端 Search 共用专用 ID 高水位；observer 另占区间。来源退休立即移除旧按钮，重绑同源、B 更新都不能重置 A pending。
- ComponentDemo 消费原 host packet，组字期间不发 Retry 命令；原生宿主将携带原 key 的命令同步交回原 Single/Multi binding。更新继续来自原 watch，不另建恢复状态机。
- 独立审查发现的泛化错误隐藏已修正：通用 Sidebar 始终保留调用方提供的 inventory error。仅同一原 packet 的错误在 demo 被 active observer diagnostics 覆盖时去重，fixture 新增的错误仍显示。

## 分层验证

| 范围 | 结果与边界 |
| --- | --- |
| UI workspace | 675 passed / 0 failed / 1 既有忽略（TekesUI #49），96 组结果；随后仅做语义不变的 Clippy if 合并和测试 helper 参数整理 |
| 最终组件库 | 上述整理后定向重跑 10 passed / 0 failed；未把早期失败日志改成成功 |
| presentation | 240 passed / 0 failed；25 组结果，包含新增 opaque key/结构校验 |
| 严格检查 | UI 与 presentation all-target Clippy、两个 workspace fmt、边界检查通过；SDK 243 份输入重新 hash 通过 |
| 宿主 | 普通 demo build 成功；组件宿主测试消费真实 fixture packet、拒绝旧 host owner/外来 key，保留独立投影错误 |
| 第二 consumer | 独立外部 package 实际离线锁解析、编译并运行，构造 synthetic observer input，更新 Sidebar、挂载独立状态控件；这不证明有效 binding 恢复 IO |
| 有效恢复 | binding 内部 cfg(test) 的终态注入后，生产 SharedAgent.follow 在真实 fixture runtime 恢复；覆盖旧键、替换 binding、跨端 owner、健康端继续与旧代永久拒绝 |
| 几何 | Scene/ContentHost 测试证明 search/observer 不重叠，72px 用例的整个 Retry button 可见；不扩报任意极小窗口或全部端点按钮可达 |

控件点击契约、binding 有效恢复、宿主命令接线分别有证据；没有运行真实窗口点击或真实 provider 断流后的 UI 恢复闭环。ClientKit 当前没有 inventory observer 专用 Retry，这项是 AgentKit 的 observer-only 交互，不能称作已有 ClientKit 完整 parity。

证据根 `.runtime-fixtures/sidebar-host-20261024/`，最终机器收据见[JSON](rust-agent-ui-sidebar-observer-retry-results-20261024.json)。`observer-ui-source-manifest.json` 记录源码与归档程序 hash；demo/consumer 分别归档为 `agentkit-sidebar-observer-ui-demo`、`agentkit-public-observer-ui-consumer`。完整 suite 早期的 fixture revision、固定 ID 冲突失败和初始 Clippy 失败日志全部保留。新收据不覆盖此前 `3d594a8` 的 664 项收据。

## CI 与剩余范围

前一 head `2341748` 的 PR [40058315999](https://github.com/TekesApps/TekesAgentKit/actions/runs/40058315999) 已完成：headless、UI fmt/Clippy/workspace tests、独立 consumer 成功，严格 nested detail reading 失败，后续 demo build 跳过。仅取得 jobs/steps，未取得本 run raw 阅读 JSON，不借用旧 head 的 offset。`be4fc58` 和后续提交须看自己的 CI。

本批仍不关闭 #22、不合并/打稳定 tag。Mirror FTS/pins 策略、远端 cursor、真实多 harness 故障、极小窗口全动作可达、分页/reveal 像素、桌面、物理中文 IME、AX、ProcessContainer 继续开放；TekesUI #53 的严格门槛保留。

全部 Cargo 验证复用 `/tmp/agentkit-layout-text-292-target`，没有创建 target。归档的 demo/consumer 共 99,270,304 bytes，动态依赖为系统框架/库。没有删除共享产物；释放空间 **0 bytes**。当前验证缓存、用途尚不确定的共享变体、源码/WIP、固定依赖快照和历史证据均保留，没有把旧清理估计计入本轮。

共享 target 本次 `du -sh` 约 37 GiB；包含现用构建和不同 feature/profile 的历史变体。尚未逐一证明可删除的变体继续保留，不因体积大整删，也不把保留判定扩报为完成全部历史缓存审计。
