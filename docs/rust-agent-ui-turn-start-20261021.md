# 普通回合 Start：588277 的接收侧组件验证

在已提交的实际回合轨道基础上，独立消费 TekesUI #28/#32 的固定候选 `5882774a0e73dfcb423efb4260c150e03f50f5cc9a87bd2331c1e171963faacd`。归档 SHA256 为 `1d4a3541ba3bc8127b21d6dd8d69ef9608583c72a5287ced547078ccdec735d4`，490 文件，在未改 c779 上恰好9路径增量。接收侧解压逐文件重验，tokens/sets/api 值与旧输入一致；旧 292/10b5/c779 和默认公开 pin 均不变。

普通回合从原轨道按钮捕获 `RunTurnNavigationKind::TurnStart`，第二公开 consumer 认领原手势后，仍只调用原 `ContentHost::reveal_scroll_reading_item`，placement 为 `Start(extended::Axis::Vertical)`、默认零 margins，使用原 owner/version/item/renderer。AgentKit 不写滚动 offset，不反算补偿，不借焦点、不重建编辑器。最后回合继续使用既有 Latest 语义。

新增的两个 actual Run 测例验证：

- 原轨道 PointerDown 捕获普通回合，按真实祖先 clip150 的可见起端对齐；PointerUp 不重复派发，clone 再次认领拒绝。原 Composer、focus、合成 preedit、输入 baseline 和横向偏移保持。
- 原输入行高于 viewport，且已经部分可见时，Start 仍将行顶端移动到实际可见起端；Centered 只用来安排该测试的前置位置，不用作普通回合实现。

`--append-rail-probe` 将未修改的8项 Run 事务与6项实际 rail 检查置于显式 Start 扩展之前。调用方式与[轨道消费说明](rust-agent-ui-turn-rail-controls-20261021.md)相同，改用此候选 source/manifest，并使用：

```sh
--probe scripts/probes/agentkit-turn-start-candidate.rs \
--append-run-probe --append-rail-probe --turn-rail
```

该次接收侧整包364项、16项 Run/rail/Start 测试、第二公开 consumer35项、全目标 strict Clippy 和原生构建通过。SDK controls/text 执行437项（text按名称过滤1项GPU测试，不是全GPU矩阵），独立SDK consumer224项；SDK三包 strict Clippy/native example build 与解压内依赖闭包均独立复验。所有490源/锁哈希保持不变，不复用 producer 的原生/GPU结论。

这还不是588277的原生普通回合验收。已完成的原生 Latest 轨道检查来自先前 c779 的精确二进制，不能转记到新身份。Start 的原生按钮、动画/RM、wheel竞争、真实边界clamp、原生选区/undo和完整分页/三端闭环仍要继续；普通回合原生宿主 adapter 尚未接入。#33 的业务包装焦点绘制层接口也仍未交付，宿主导航历史回调、hover/ramp/preview与其余 Views 继续列为未完成。先前一次原因未确认的原生末尾publication变化也不能被本轮API通过覆盖。

默认8cf公开依赖不变，UI仍为实验包，不建议提升冻结；本轮不构成合并/tag或完整目标完成的依据。同名JSON记录精确输入和实际检查日志。

后续[主导航宿主回调与588277原生Start](rust-agent-ui-turn-navigation-host-20261021.md)已增加显式显示事实、原点击历史捕获和原生组字内选区检查；此前收据保持原范围，完整交付仍未完成。
