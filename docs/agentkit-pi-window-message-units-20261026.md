# Pi 恢复窗口的消息单位：#68

阶段范围仍为5b，#67之后新增的实际阻断修复；不扩产品功能。

固定Pi1.3.0 / Node24.20.0的真实诊断在product4274上失败：公开page20后，同一pointer/inventory的原历史窗口从98 entries扩大到231；再次公开关闭/打开后到274，Earlier提前耗尽。正式性能样本0，原负证据见[绑定](evidence-bindings-20261026/issue68-pi-window-negative.json)。

`SessionRuntime::add_journal`将`NativeConversationSnapshot.entries.len()`交给`SessionConversation.max_messages`。Pi公开`active_branch`按`type=message`计数，原entries还含custom metadata；23c4647保留已分页窗口的意图正确，但使用了不同单位。ClientKit冻结c289bc9的`PIProjectedFeed`也保持显式消息limit、Earlier仅增加该limit，没有把所有entries总数充当消息limit。

最小修复只统计原message entries，仍保留min50和已分页大窗口，不改Pi、Kernel、Swift或SDK。恢复的原ID、内容、顺序不变；测试端点带真实message/custom ancestry，并按消息裁窗口，覆盖初始50、已分页100、重连及公开close/open，旧全message测试保留。

最初实现阶段尚未取得定向Rust/实际Pi新程序收据；当时状态进行中，未关闭#68/#55/#56。后续精确验证与独立集成见下文。主会话将在固定输入和SSD共享target上串行验证。原9 WIP保留。

## 新精确程序验证

98b1d9a / headless SHA18d8f643（feature observer）在固定Pi1.3.0/Node24.20.0真实回归通过：两会话各50 message+73 custom，重连/close-open保持；加载到100 message+133 custom后同样保持。原ID、完整entry内容、顺序、pointer与epoch均核对；原窗口膨胀负证据未改写。session10测试、session/headless严格all-targets Clippy通过，独立源码review无阻断。见正证据绑定；该验证时间点#68仍OPEN、已验证待集成。随后独立集成已完成，见下节。正式性能样本仍0。

## 进入main并关闭

独立两文件PR69的精确head4c682b1已另编CLI66a2c9b8，固定Pi真实同范围验收通过；独立公共consumer覆盖双lease、分页、重连、释放/重新取得的100消息窗口。既有CI全绿，合main1dcc521后CI40227909675成功，文件树与4c候选相等。#68于2026-10-25T19:54:43Z（北京时间10-26）关闭。仅此票达标；原PR10 UI及完整阶段未通过，未发布tag。
