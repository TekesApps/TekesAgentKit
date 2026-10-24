# PR #10 / 9b9ec1f CI 结果（2026-10-24）

[PR run 40073869211](https://github.com/TekesApps/TekesAgentKit/actions/runs/40073869211) 与 [push run 40073865859](https://github.com/TekesApps/TekesAgentKit/actions/runs/40073865859) 均已 terminal。两者 headless检查通过；UI pin/格式/严格Clippy/workspace测试和实际独立public consumer执行通过，唯一失败步骤是严格 nested detail reading。普通demo build在CI中因前一步失败而skipped；本地9b的普通build通过证据另列，不能称CI该步骤通过。

已下载两份当前raw artifact，均为 accepted=false、parentFollowingAfterInnerUp=true、parentOffsetBefore=0、parentOffsetAfterScroll=0、parentOffsetAfterAppend=634、innerOffset=100、detailVisibleAfterAppend=false。没有引用旧head结果来替代本次原始观察。#53继续失败，不合并/打tag/发布。

PR实际checkout为synthetic merge `3b1d189a483b581ff0185e565a111b14f75a6cb4`，push checkout为`9b9ec1fbb06bd02809d72a338dec02b5b6c4d520`；两份consumer输入均136文件、SHA `ca4730ce8af3bc488c8e3c3862e45380387b09486b747a6b070b56cb902293f3` 且matches_git_head=true。这是public consumer的路径依赖closure，不代表整个repo。prepare receipt自身的cargo_invoked=false只描述准备步骤，consumer实际执行成功以workflow的独立步骤为证，二者不混称。

[机器收据](rust-agent-ui-ci-9b9ec1f-20261024.json)保存两份raw结果、checkout、输入closure、步骤结论与artifact SHA。原文件在SSD `.runtime-fixtures/real-multi-sidebar-20261024/ci-400738{69211,65859}-artifacts/`。该结论只适用于9b9ec1f，不自动转移到之后提交。
