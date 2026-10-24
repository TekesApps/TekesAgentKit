# #17 / #23：Native 有界 Task-source 合成 turn 终态

基线 `20b4c44`。只读 Kernel **v0.9.3 tag `09508a9083c24c7accefc99516739f279b85913e`** 的 `crates/supervisor/src/bounded_child_history.rs`：公开 `sourceCall.phase` 映射为 completed settle→done、interrupted→stopped、其他settle→error、未settle→running。当前AgentKit保留tool.phase，却把合成turn的非running全标completed，错误和取消在turn层丢失。

本轮只修明确已定义的 phase：running→running、done→completed、error→error、stopped→aborted。error/aborted采用AgentKit既有Native turn命名；不改变未知phase的兼容fallback，不从child entries、父Run结算或run_open猜来源phase。不改source call原身份、arguments、tool.phase、历史、指针、权限或协议。

先通过现有公开SEP fixture与 `ChildRun::open(SameSession)` / `transcript` 的原读取路径验证error/stopped回归会失败，再改映射。核对running/done保持、同source身份及原tool.phase/arguments、child entries独立保留；不能只测试一个复制的映射函数。代码只限AgentKit presentation；ClientKit/Tekes/Kernel/TekesUI只读，不新增Swift或基础SDK改动。

验证使用SSD共享target，串行跑有意义的定向红/绿、root相关回归/严格检查、Rust UI宿主与独立公开consumer；保留失败、精确源码和程序。无新target、桌面、物理IME、OS AX或权限提示。真实Native同原pane的running→failed来源见证仍是后续独立工作，不能以公开fixture/已发布Kernel源码规则替代。#17/#23不自动关闭，#53严格阅读不豁免，不合并/打tag/发布。
