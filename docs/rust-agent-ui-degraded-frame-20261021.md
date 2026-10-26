# 历史投影失败时保留归档、目标与 epoch 语义

修复 ClientKit 显示对齐审查发现的 controller 降级路径。ClientKit 的 `ThreadStatusPolicy.acceptsInput` 从线程生命周期判断归档，不因历史解析失败而把归档改成可写状态。Rust 现有错误分支只克隆旧快照，覆盖 phase 为 Unavailable，且未更新 connection epoch；换目标失败时还保留了旧提交回执。

`Shared.refresh` 现在在同一次 `SessionRuntime::inspect` 中调用私有 `refreshed_snapshot`：成功继续使用原 presentation；失败仍关闭提交/取消并标记历史 stale，但 Run 生命周期由该次 authority 读取。当前 summary 或 workspace archive 集合确认归档，即保留 Archived，包含归档与坏历史首次同时到达的情况。显式取消归档后，坏历史仍显示 Unavailable；只有明确的新历史版本恢复，才由正常 refresh 恢复 Interrupted/Idle 与其真实权限。不复制一套状态机，不增加 wire 字段或公共 API，也不重试写操作。

降级 publication 使用同一 authority 的 connection epoch。跨 epoch 不沿用旧 child availability；换目标/selection generation 后清除旧 Transcript、Summary、submissions、events，使用新目标的 scope 与已有草稿。保留的同目标历史明确 stale，错误说明仍可见。独立任务、子代理、SideChat 使用同一个 controller 路径，不由宿主猜测归档或修正快照。

三项控制器回归先在旧分支下失败，覆盖：

1. summary/workspace 两种归档事实、坏历史与归档同时到达、持续失败、显式取消归档、带新 historyRevision 的恢复。
2. A 有已准备提交与后续草稿，切到坏历史的 B：A 回执与历史不进入 B，双方草稿保留。
3. 失败恰逢新 connection epoch：快照带当前 epoch，并撤销旧 child availability。

初始测试曾使用不存在的 PromptMode::Enqueue；改为公开 Queue。第一次恢复 fixture 没有更换 historyRevision，合法的前缀合并保留了坏尾部；现在按公开历史替换契约提供新版本。准备提交会清空已提交草稿，因此目标隔离 fixture 明确在 prepare 后写入 A 的下一份草稿。保留初始失败日志，未放松归档、权限、目标或 epoch 断言。

新增原生模式：

```sh
ui/target/debug/tekes-agent-ui-demo --components --transcript-content --composer-projection-failure-selftest
```

它只使用 owned local SEP fixture。fixture 发布协议有效但 promoted message 缺失的 journal 事件，经真实 SessionRuntime → RunController → RunView，断言公共错误说明含 `promoted assistant is outside loaded history`。原生窗口验证 preedit、坏历史降级、归档隐藏及 AX/焦点退出、取消归档后仍不可提交、明确修复 historyRevision 并重连恢复。原编辑器、本地中文文本、B 草稿、消息数保持；不执行真实模型调用。普通归档恢复模式继续使用同一探针，故障注入只在新 CLI 模式启用。

分层证据见[JSON收据](rust-agent-ui-degraded-frame-20261021.json)。这关闭了[可见性记录](rust-agent-ui-composer-visibility-20261021.md)中的 controller 降级缺口。它不是三端真实故障/阅读、实体 IME、VoiceOver、完整 RunView 叠层或最终布局验收。UI仍保持实验状态，默认 TekesUI公开固定 pin 不变。
