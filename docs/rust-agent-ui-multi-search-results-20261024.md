# 多端 provider 搜索交付收据

关联 #22、#15、#27、#28，PR #10。共享协调器基线 `cd3d486`，后续集成提交见本文件 git 历史。只消费精确 TekesUI `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`；SDK、ClientKit、Tekes、Kernel 未修改。

## 已实现

- 固定 endpoint 集合复用原单端搜索 reader：独立 owner/query generation、ABA、clear/drop、原端重连与双 key 重试门禁。
- 公开 bindings 使用原 inventory 按完整地址 join；局部失败、unsupported、hasMore、unresolved 保留原端语义，不制造全局 single key。
- Sidebar 组合 SDK Text/Button 的业务状态子组件；同 attempt 保留控件与 pending，更换来源退休旧 raw action ID。Single/Multi 往返也不重用历史 Retry ID。
- 组件 demo 消费共享投影；真实配置可逐项注册 Native/Pi/DSH connector，单端专属 proof 拒绝多端配置。没有克隆首端假装多个后端。

## 实际验证

| 层级 | 结果与范围 |
| --- | --- |
| presentation | 最终完整 suite 234 passed / 0 failed / 0 ignored；含 7 项双端协议测试；all-target 严格 Clippy 通过。 |
| UI workspace | 653 passed / 0 failed / 1 项既有忽略（#49）；all-target 严格 Clippy 通过。 |
| 独立 component consumer | 实际编译运行并挂载 RunPort、Sidebar、SidePanel 与 SidebarMultiSearchStatus；不是业务交互验收。 |
| 独立 binding consumer | 公开 API 实际编译运行，验证无效配置拒绝；成功 IO 由协议 fixture 另证，不扩大 consumer 运行范围。 |
| 固定 SDK | 6 个包精确 pin、243 份审查源码 hash 一致。 |
| 自有离屏 renderer | 双端成功、B 端失败各运行退出 0，检查 PNG；挂载 TranscriptContent，成功端结果保留，B 端独立 Retry。没有桌面截图或系统输入。 |

证据根目录 `.runtime-fixtures/sidebar-host-20261024/`：`presentation-multi-search-final-suite.log`、`ui-multi-search-workspace-tests.log`、`presentation-multi-search-validated-clippy.log`、`ui-multi-search-validated-clippy.log`、`multi-search-public-consumers-result.json`、`multi-search-offscreen-result.json`。验收程序 `agentkit-multi-sidebar-demo`、构建输入 `multi-search-source-manifest.json` 保留准确的 commit + WIP 来源和 SHA256；程序身份不转移到后续源码。

之前推送的 `3b08501` CI：PR run [40052104853](https://github.com/TekesApps/TekesAgentKit/actions/runs/40052104853)、push run [40052101100](https://github.com/TekesApps/TekesAgentKit/actions/runs/40052101100) 都已完成；headless、UI suite、独立 consumer 通过，仅严格 nested detail reading 失败，demo build 后续被跳过。原始结果仍为 `accepted=false`、`parentFollowingAfterInnerUp=true`、追加后父 offset 634、detail 不可见，属于 TekesUI #53。未放宽门槛。本批 demo 已本地正常 build；新提交必须看自己的 CI。

## 仍未完成

Mirror 本地 FTS 与 pinned-search 语义对齐、远端 cursor 协议、真实多 harness 组合、非首端 SideChat、observer 同实例恢复、分页/reveal 实际像素可见性、桌面/中文物理 IME/AX/ProcessContainer 和完整 ClientKit parity 继续开放。provider hasMore 不代表有公开 cursor，不能重复 query 冒充下一页。

所有构建复用已有 `/tmp/agentkit-layout-text-292-target`；本批没有新 target、缓存删除或释放空间。保留历史证据、快照和 WIP。离屏 PNG 由自有 renderer 导出，不申请 Screen Recording。
