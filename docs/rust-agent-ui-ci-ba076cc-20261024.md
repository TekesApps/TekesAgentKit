# PR #10 / ba076cc CI 终态

[PR 40098659285](https://github.com/TekesApps/TekesAgentKit/actions/runs/40098659285) 与 [push 40098655443](https://github.com/TekesApps/TekesAgentKit/actions/runs/40098655443) 均已结束。checks、UI workspace/严格检查和独立 public API consumer 执行通过；两次唯一失败均为严格 nested detail reading，后续普通 demo build 被跳过。不能把本地 demo 的既有成功称为本次 CI 构建成功。

两份原 artifact 均为 accepted=false、parentOffsetAfterAppend=634、detailVisibleAfterAppend=false。#53 门槛保持，不合并/打 tag/发布。未发现 ba076cc 新增的 consumer 或 AgentKit 测试失败。

PR 实际 checkout 为 synthetic merge 0dc0654fb49a413aab24d707940d3222de846ccc，API 原提交父列表含 ba076cc；push checkout 为 ba076cc。两份 consumer 输入均为137文件、e216110c5e2c26b279f832c9c2cbfa596f004f640b23cf6560b0acfb743e4466、matches_git_head=true；SDK 243文件闭包为3ad8aa98a567679b775e78b634f45888c9eb70ffd05708a4f781c0ce7264471c。prepare 收据只证明准备，实际 consumer 执行结论来自成功的 workflow 步骤。

[机器收据](rust-agent-ui-ci-ba076cc-20261024.json)列出原 artifact 摘要与步骤结论；证据保存在 SSD `.runtime-fixtures/child-transcript-source-20261024/ci-400986{59285,55443}/`。此结论仅适用于 ba076cc，新提交单独检查。
