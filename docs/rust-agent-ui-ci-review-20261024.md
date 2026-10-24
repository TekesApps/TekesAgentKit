# PR #10 最新 UI CI 复核

关联 AgentKit #15、#18、#27、#28；复核日期 2026-10-24，PR head `32986d4d000ac1fa4470029e42a44c1878b70fd9`。

| 检查 | pull_request [40044117789](https://github.com/TekesApps/TekesAgentKit/actions/runs/40044117789) | push [40044114325](https://github.com/TekesApps/TekesAgentKit/actions/runs/40044114325) |
| --- | --- | --- |
| headless checks | success | success |
| 固定公开 pin 核验 | success | success |
| UI fmt / Clippy / workspace tests | success | success |
| 新增 standalone public API consumer | **success** | **success** |
| 严格 nested detail reading | failure | failure |
| 后续 demo build 步骤 | skipped | skipped |

两次独立 consumer 步骤均实际执行锁解析及 `cargo run --locked --offline`，不是只生成项目；仅证明三个公开业务组件的构造、合法空输入更新和 ContentHost 挂载。未引入新的失败；不把构造验证扩大为完整渲染、业务交互或系统 IME 验收。

PR checkout 是 GitHub 合并测试提交 `35969c3639582d870d5724de052ec144d8eaaeb2`，父提交为 main `f1daa0ffcf6ced41e21f351e22845455ca78006c` 和上述 PR head。push checkout 则为 PR head；生成器的 `agentkit_head` 如实记录各自 checkout，不能把合并测试提交误报成业务修改。

两份严格阅读原始结果一致：`accepted=false`、`parentFollowingAfterInnerUp=true`、`parentOffsetAfterAppend=634`、`innerOffset=100`、`detailVisibleAfterAppend=false`。失败仍是 TekesUI #53 的父容器追加跟随问题。没有放宽门槛、跳过断言或修改 SDK；UI job 整体仍红，不能合并/打完成 tag。

证据及哈希见[机器收据](rust-agent-ui-ci-review-20261024.json)。两套 job 元数据、生成器收据、consumer 清单/锁/源码、严格阅读原始 JSON 已归档到 `.runtime-fixtures/issue-batch-20261024/ci-40044117789*` 与 `ci-40044114325*`。生成器的 `prepared-not-built` 是步骤开始时的准备收据，实际编译运行结论取对应 CI step，保留原文件不改写。

本轮未启动桌面、截图、系统权限、签名安装或原生窗口；没有创建 target，也没有删除缓存，释放空间为 0。
