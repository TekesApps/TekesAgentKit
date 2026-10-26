# TekesUI #17 第一批来源基础（2026-10-18）

当前显式开发快照：`89b93cc78b49213498fa4977e1270fa6c2129cdc847e1d808ad27d0a8dc994b0`；不是公开不可变发布 pin。

新增可选 `Window::set_text_key_events(true)` / `Event::TextKey(Started/Finished)`，观察原始 KeyEvent、组字开始状态与回调分发结果。原有文本回调不重复，默认宿主行为保留。CommandDispatched 不等同编辑成功；本批还不能直接配置 Composer 提交策略。AgentKit 暂未启用来源模式或改变业务提交状态。

本修复完整验证：UI workspace13项、默认headless76项、严格Clippy和Rust边界通过；独立公共consumer57项，Gallery真实字符/Enter/Command/Backtab链及GPU截图另存于TekesUI `design/reviews/text-key-context-20261018`。第一批6路径补丁附独立WIP依赖归档；脱离checkout的runtime395项回归与consumer all-targets编译通过。

依赖顺序：来源基础 → #17公共编辑确认和组字/Tab/换行/提交意图 → #18受控编辑事务与#20视口 → #21动作目标/来源；#15/#16/#19已有实现复用。完整#15–#21交付尚未就绪，之后才移交本会话真实体验验收。未运行AgentKit原生UI、物理中文IME、VoiceOver或性能，未提交/推送/关闭Issue。
