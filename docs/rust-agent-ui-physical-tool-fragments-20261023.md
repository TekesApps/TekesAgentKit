# 原工具组的物理时间线分区

对照当前 ClientKit `TranscriptMetalActivity.swift` 的逐 call 条目、连续组和输入间历史分区，
本阶段将原 OutputMessage 的工具组拆成独立阅读行，并消除迟到工具引用的重复控件。
仍只消费公开 TekesUI `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`；未修改上游、Tekes host、ClientKit 或 Kernel。

## 公开接入契约

- 新增 `TranscriptContentRowKey::MessageToolFragment(owner, key)` 与 `OutputMessageToolFragmentKey { tool_id, block_index }`。
  唯一 call 用原 tool ID；只有重复引用无法唯一归属时才保留实际源 block index，不用时间或 UI 下标生成身份。
- `OutputMessageFragment::tool_key()` 标识原工具分区，`kind()` 仍为 Tools；原 part/详情/复制控件保持各自 ID。
- `row_keys` 包含每个当前挂载的原分区，`row_id` 是其 SDK reading item。旧 `MessageFragment(.., Tools)`
  只兼容查找第一个当前工具分区，不再表示整个工具集合；宿主应遍历新 key。穷尽 match 需补新 variant。
- 每个工具分区需要一个独立 namespace，prepared plan 包含所有隐藏/可见分区的预算与预测，
  容量不足或旧计划会在树修改前拒绝。宿主继续使用原 reading transaction，不计算 offset。
- frame、fragment 和 block key 都是 view-local 布局/来源信息，不是 session pointer、持久身份或提交权限。
  新 API 尚未发布，不能只固定 v0.3.8；开发宿主需固定包含本阶段的完整 AgentKit revision。

## 原节点与来源所有权

原工具组及其 OutputPart/SDK 文本控件实际移到独立行。更新前，OutputMessage 检查每个 fragment 的
owner/scope、唯一根 ID 和原直接子节点 ID 清单，完整归还后使用原控制器更新，再拆出新分区。
计划、preflight 和原节点构建共用同一连续组算法，思考模式的节点选择也参与同一事实投影。
不会克隆 ToolActivity、丢失原隐藏详情，或由宿主另存工具/滚动状态机。

较早的独立调用被迟到 assistant 再次引用时，原 ToolActivity 才是调用的原来源控件；
Transcript 不再从引用生成第二套 OutputPart。有明确 source position 时，工具必须与原消息/block 对应
才由该消息持有；未知来源保持原显式处理。独立 OutputMessage 在没有完整 Transcript 的场景下
仍可展示其引用详情。同一控制器使用来源显示配置，不改写 DTO、原位置或复制权限。

## 验证层级

完整 UI **612 通过 / 1 原有忽略**，严格 all-target Clippy、格式和 Rust 边界检查通过。
根 Rust/TOML/lock 与上一阶段哈希一致，保留此前 310/严格 Clippy 证据，没有重复根构建。
固定标准 journal 分别覆盖消息内原调用、跨追加输入的 a/bc 组，以及较早独立调用的迟到引用。
第二 consumer 验证原物理行、两个输入段计数、折叠/展开/重开、原组和 disclosure 对象保留，
缺失子节点拒绝 preflight、恢复后不重建，以及 None/Compact/None 的计划与原 part 一致性。

CoreText/原 SDK reading transaction 验证实际布局 a → 追加输入 → bc，
工具结果更新及在工具前插入旁白、改变 block index 后，原工具组仍是 Preserved 锚点、位置误差 ≤1px。
原 group/part owner、Composer 焦点、中文 preedit 与 baseline 保留，旧 SDK token 被拒绝。
只读 pane 使用自己的 900/901 publication 和本地段选择，没有 Composer 或反馈能力。
这些不是原生窗口、物理键盘、候选窗或 VoiceOver 证据。
同一冻结 UI 程序配已验证未变的 headless，通过真实 Pi 1.3.0、DSH 0.2.1-rc.2、
Native Kernel 0.9.3 UI-only 的协议、历史分页与离屏 GPU 路径。原 complete/rich/native false 保留，
真实 provider 场景没有生成本阶段混合工具 fixture，不能替代专门组件回归或原生分页按钮验收。

程序、原始失败/通过日志、固定快照和差异在 `.runtime-fixtures/physical-tool-fragments-20261023/`；
精确身份和完整范围见同名 JSON。本轮未新建 target、未删除证据或验收程序。

## 尚未交付

跨不同消息 owner 的全局连续工具合组，以及 reasoning/prose/tool 的完整同内容配对布局仍需继续对齐。
本阶段不能单独证明 completeTranscript/completeRunPort、富内容或完整 ClientKit View parity。
公开 main 仍为精确 pin；#48 正式 ProcessContainer、#53 嵌套阅读仍开放。
7903f24 的两项核心 CI 成功、两项 UI 在 #53 严格步骤失败（两个官方 job API 已核对），门禁保留。
当前冻结程序没有重试原生激活失败；原生/IME/分页/重连验收仍待完成。PR #10 保持 draft，不合并或打完成 tag。
