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

## Sidebar 宿主接入前的再次复核（ddc49c7）

新基线 `ddc49c75bcd2cf4be4dd17152db7d595e6ed65da` 的 PR [40048515783](https://github.com/TekesApps/TekesAgentKit/actions/runs/40048515783) 与 push [40048512623](https://github.com/TekesApps/TekesAgentKit/actions/runs/40048512623) 均已完成：headless、UI workspace/格式/Clippy、独立 consumer 全通过；只有严格 nested-detail reading 失败，后续 demo build 被跳过。两份原始 JSON 的失败值与上表同型，没有新增失败或豁免门槛。精确 jobs/steps、原始失败哈希见[本批 CI 收据](rust-agent-ui-sidebar-host-ci-20261024.json)。这些结果只适用于 ddc49c7；随后新提交须等各自 CI，不能借用此前绿色步骤。

## 多端搜索接线 df5429f 的最终 CI

PR [40054890642](https://github.com/TekesApps/TekesAgentKit/actions/runs/40054890642) 与 push [40054887302](https://github.com/TekesApps/TekesAgentKit/actions/runs/40054887302) 均已完成。headless、UI fmt/严格 Clippy/完整 suite、新增独立 public consumer 全部成功；仍仅严格 nested detail reading 失败，后续 demo build 跳过。独立 consumer 已包含新搜索状态子组件。

PR 下载原始 `agentkit-nested-detail-reading.json` 仍为 `accepted=false`、`parentFollowingAfterInnerUp=true`、父追加 offset 634、inner offset 100、`detailVisibleAfterAppend=false`。原始 artifact 和两套 jobs 归档于 `.runtime-fixtures/sidebar-host-20261024/ci-40054890642/`、`ci-df5429f-{pr,push}-final-jobs.json`；仍跟踪 TekesUI #53，不放宽门槛、不据此合并或打稳定 tag。后续 SideChat 源码不借用本次 head 的 CI 结果。

## 多端 SideChat fa9c158 的最终 CI

PR [40055941110](https://github.com/TekesApps/TekesAgentKit/actions/runs/40055941110) 和 push [40055938134](https://github.com/TekesApps/TekesAgentKit/actions/runs/40055938134) 均完成：headless 与 UI 格式/严格 Clippy/完整 suite、独立 consumer 成功，仅严格 nested detail reading 失败，demo build 后续跳过。完整 jobs 元数据保留为 `ci-fa9c158-{pr,push}-final-jobs.json`。

该 head 的 raw artifact 下载遇到网络 EOF，未取得本次原始阅读 JSON；上述结论依据实际 steps，未把 df5429f 的数值当成本次重新测得的数据。仍保留严格门槛，跟踪 TekesUI #53；后续 observer API 提交另看自己的 CI。

## Observer 恢复 532dd50 的最终 PR CI

PR [40057616124](https://github.com/TekesApps/TekesAgentKit/actions/runs/40057616124) 已完成：headless checks、UI 格式/严格 Clippy/workspace tests、独立 public API consumer 成功；严格 nested detail reading 失败，后续 demo build 跳过。完整 jobs/steps 归档为 `.runtime-fixtures/sidebar-host-20261024/ci-532dd50-pr-final-jobs.json`。

这是 `532dd50` 本身的 step 结论，未取得该 run 原始阅读 JSON，不能沿用此前 head 的 offset 数值作为本次观测。UI job 整体仍失败，保持 TekesUI #53 的严格门槛，不合并或打稳定 tag；本地普通 demo build 和 reader 生命周期测试与 CI/桌面验收分别记录。


## Observer Retry UI 开发前的 2341748 CI

PR [40058315999](https://github.com/TekesApps/TekesAgentKit/actions/runs/40058315999) 已完成，headless、UI fmt/Clippy/workspace tests、独立 consumer 成功，严格 nested detail reading 失败，后续 demo build 跳过。元数据归档 `ci-2341748-pr-jobs.json`。未取得本 run raw 阅读 JSON，不复用旧数值；后续 `be4fc58` 用户入口实现另看自己的 CI。


## Observer 用户入口 9b1d263 CI

PR [40059408663](https://github.com/TekesApps/TekesAgentKit/actions/runs/40059408663) 完成，headless、UI 格式/严格 Clippy/完整 suite、独立 consumer 成功，严格 nested detail reading 失败；后续 demo build 跳过。元数据 `ci-9b1d263-pr-final-jobs.json` 保留，没有本次 raw 阅读 JSON，未借用旧数值。后续 query/folder membership 的 `147642b` 另看自己的 CI。
