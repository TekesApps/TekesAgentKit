# #22/#23/#25 续批交付（2026-10-24）

主会话审查/集成与串行验证；新子会话分别负责语料源、binding/宿主传输、排名投影、SidePanel 回归与独立审查。没有联系已停止的旧会话。

- **#22 `3aaee78`**：同一库存快照发布全部已保留且有 summary 的名称，带固定注册端点域和 connection epoch；Single/Multi 原始 packet 与 demo 保留该来源。完整域的 SQLite FTS5 bm25 只在 UI binding 计算，实际消费组件现有 sort，再交给组件 pins/标题/目录/分页；provider-only 使用 1000000。部分域保留已知名称/provider 成员，拒绝宣称完整排名或权威本地空态。新 frame 缺语料时不拼旧 corpus。额外修正搜索结果无限展开为按本地页大小截断，搜索不补插 selected。见[契约及保留生命周期边界](rust-agent-ui-sidebar-ranked-search-20261024.md)。
- **#23 `adbb7f3`**：原生宿主子会话打开错误分支与直接回归共用原 request-current 判断。当前请求产生 notice，切换父会话后的旧请求被忽略；没有新状态机或原生窗口操作。
- **#25**：独立只读复核没有新增不依赖 SDK/桌面的确定实现缺口；修正文档中已被 `6420876` 替代的“缺 usage 隐藏整个 section”描述。剩余状态附件/交互呈现跟踪 TekesUI #47，完整布局与原生/AX 仍开放。
- **关联 #29 `36f0668`**：固定公开 Native/DSH producer/schema 和真实 Rust adapter 的本地 wire 回归已核验，见[收据](rust-agent-ui-membership-wire-results-20261024.json)。源码/协议 mock 不等于本机真实 harness 重跑。

## 分层验证

| 检查 | 结果 |
| --- | --- |
| UI workspace，100 result groups | 706 passed / 0 failed / 1 既有 #49 ignore |
| presentation + web 适用全部 tests | 258 passed / 0 failed / 0 ignored |
| 分页/双端排名/语料/ChildOpened 定向 | 21 passed / 0 failed / 0 ignored |
| Python 工具回归 | 92 passed |
| root 与 UI workspace all-target strict Clippy、fmt、边界 | 通过 |
| 普通 demo | 构建通过，未启动窗口 |
| 独立 workspace public consumer | 离线锁解析、编译和运行通过；执行四条公开 projection/component 断言 |

双端回归通过真正的 SharedAgent follow 与独立 fixture runtimes 产生库存，核对同 ID、隐藏子会话、全局排名、分页和缺失语料的已知成员保留。单端 public projection/组件回归核对 local/provider 并集、固定语料、目录/页、pin/title 及未完整空态。独立 consumer 的覆盖限于这四条公开 API 断言，不冒充真实 harness 或原生窗口。

完整日志、独立 consumer 源码/锁、普通 demo 与 consumer 可执行文件及 dylib 清单保留在 `.runtime-fixtures/sidebar-host-20261024/`；哈希及精确 source commit 见[机器收据](rust-agent-ui-sidebar-ranked-results-20261024.json)。初轮失败日志保留，最终结论仅取 final/reviewed 日志。源码、WIP、固定 SDK 快照和历史验收程序均保留。本批复用共享 target，未新建 target、未删除缓存；实际释放空间 **0 bytes**，不沿用此前预计或释放值。可执行文件以 APFS clone 归档，仅 consumer 执行，没有为历史 demo 创建/启动新应用身份。

## CI 与未验收项

开始本批时，`532150e` PR/push CI 的 headless、UI suite/Clippy 和独立 consumer 全通过，仅严格 nested detail reading 失败，普通 demo 后续跳过；见[CI 记录](rust-agent-ui-ci-review-20261024.md)。这些结果不自动转给 `3aaee78` 或后续提交，新 head 单独跟踪。

TekesUI 精确公开 pin 仍为 `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`；没有改 SDK、Swift 或 Kernel。严格 TekesUI #53 门槛不豁免。实际存储保留集合/生命周期与 ClientKit 相等、所有 pins/title durable-ID/Unicode 平局、公开远端 cursor、真实多 harness 故障恢复、桌面/物理 IME/AX/ProcessContainer 均未证明。#22/#23/#25/#29 保持开放，PR #10 保持试验状态，不合并或打稳定 tag。
