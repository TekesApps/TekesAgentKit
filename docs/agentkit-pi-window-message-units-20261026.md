# Pi 恢复窗口的消息单位：#68

阶段范围仍为5b，#67之后新增的实际阻断修复；不扩产品功能。

固定Pi1.3.0 / Node24.20.0的真实诊断在product4274上失败：公开page20后，同一pointer/inventory的原历史窗口从98 entries扩大到231；再次公开关闭/打开后到274，Earlier提前耗尽。正式性能样本0，原负证据见[绑定](evidence-bindings-20261026/issue68-pi-window-negative.json)。

`SessionRuntime::add_journal`将`NativeConversationSnapshot.entries.len()`交给`SessionConversation.max_messages`。Pi公开`active_branch`按`type=message`计数，原entries还含custom metadata；23c4647保留已分页窗口的意图正确，但使用了不同单位。ClientKit冻结c289bc9的`PIProjectedFeed`也保持显式消息limit、Earlier仅增加该limit，没有把所有entries总数充当消息limit。

最小修复只统计原message entries，仍保留min50和已分页大窗口，不改Pi、Kernel、Swift或SDK。恢复的原ID、内容、顺序不变；测试端点带真实message/custom ancestry，并按消息裁窗口，覆盖初始50、已分页100、重连及公开close/open，旧全message测试保留。

本提交只表示实现可审查，尚未取得定向Rust/实际Pi新程序收据；状态仍进行中，不合main、不关闭#68/#55/#56。主会话将在固定输入和SSD共享target上串行验证。原9 WIP保留。
