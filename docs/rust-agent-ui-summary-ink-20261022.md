# Summary 普通 metadata 的语义颜色

本轮对照 ClientKit `ClientAgentSummaryView.swift` 的 `SummaryDefaultRowView.configure`、`SummaryProviders.swift` 与 Tekes `ClientAgentSummaryHostAdapter.swift`：普通 `.meta` 使用 secondary 色，标题仍为 primary。宿主只提供已有产品事实、仓库内容和导航动作；没有修改任何参考仓库、Kernel 或 TekesUI，也没有 Swift 构建步骤。

## 修正与公开契约

此前静态和可操作默认行的 metadata 都实际画成 primary；`UiCaption` 的字号角色本身不能保证 Source 所需的颜色，CompoundButton 还会传递自身前景色。第二 consumer 先记录实际原 LabelLayout/Paragraph 的绘制颜色，复现该错误，再验证修正。

现在只对 metadata 分支使用公开 `Component::child_theme` 声明当前库 palette 的 `secondary-text`。原 SDK TextContent、单行 label policy、metadata 查询 ID、原文字与 AX 内容继续保留；新增内部主题容器不接管绘制、事件、焦点、捕获或滚动。每次 paint 读取当前 palette，不缓存颜色或要求业务 publication 才切换主题。公共 Summary API 无新增字段。

`SummaryDefaultRowIds.trailing` 仍指向原 TextContent。同一 owner/action/mount 的值更新与 None↔Some 不替换原标题、metadata、控件或可选择文字。库负责字体、排版、颜色 token 和实际绘制；没有 RGB 常量、自画字形或新的状态机。

这只覆盖普通 `.meta`，不推断字符串里的 `+/-` 或 task/child 状态。Source 的 change-count 双颜色 runs、leading/status 附件及产品仓库控件仍各自待完成；不把此修正当成完整 Summary 默认行或 SidePanel 交付。

## 验证

- UI workspace496项通过，1项明确 ignored 的既有 TekesUI #49 Popover 接收门禁；strict all-target Clippy、fmt、Rust 边界与实际 Cargo 输入检查通过。仍消费公开 `09b4bab44b6dfa65487250ea816b610b9df59095`，实际8包241输入。
- 第二公开 consumer 检查所有4种主题、0.85/1/1.5/2字号及96/240/420宽度；两条原 metadata 必须实际画出，并使用库 SecondaryText。还在不更新 Summary publication 的同一挂载上切换主题，检查新颜色和原 owner。既有换行、首行对齐、动作与静态选择、publication fencing 检查保持。
- 原生 `.runtime-fixtures/AgentKitSummaryInk.app`，SHA256 `1b80159b54f74b3a1c6e92ed7522570d71650531d1a6fc896f7e1afc079ce0e1`：owned SEP fixture 经现有 SummaryLabels 提供普通 metadata，同一原子树进入 GPU 文字路径，验证实际 ink、原 metadata/动作/静态文本 owner、选择、resize/字号变化、Composer 中文 marked baseline 与无提交。原生后端的绘制收据有界且逐帧清理，不另建文本/输入状态。
- 系统截图尝试返回 `could not create image from window`，没有生成 native.png，因此没有本轮原生截图或配对视觉通过证据。保留了截图失败和前台轨迹，未用 offscreen 图片代替。该 native receipt 使用合成输入与受控 fixture，不代表实体键盘/候选窗、VoiceOver、真实三端原生 Summary 或 UI 跨进程验收。
- 同一程序的真实 Pi1.3.0、DSH0.2.1-rc.2 协议/历史/组件至 GPU offscreen 通过；Native Kernel0.9.3 独立 UI-only 复验通过。最初三端批次在 Native 因空 fixture 被 `finalAssistantItems >= 1` 拒绝；驱动原来只在反馈专项补种数据，现修正为独立 UI 快照缺少 history/search seed 时通过普通公开 prompt 准备自有最终回复。原断言不变，原失败不改判；新的 Native 复验不是原批次整体通过。Native lifecycle/archive/child 明确未运行。

早期已存在的程序名字触发防覆盖检查，未启动或修改旧程序，随后仅冻结上述新名字的一份程序。源、早期红色绘制测试、全部运行失败和最终收据保留在 `.runtime-fixtures/summary-metadata-ink-20261022/`；精确摘要见[同名 JSON](rust-agent-ui-summary-ink-20261022.json)。没有更换 SDK 或读取活动工作树作依赖，#47仍 Open；本次没有新增基础组件缺口需重复提 issue。

复用原 target，没有新增独立构建缓存；保留必要验收程序及日志，未删除用途不明确的共享产物，释放空间0。PR #10保持实验草稿，完整 Views/ProcessContainer/三端原生与配对视觉等原目标继续未完成，不据此合并或打 tag。
