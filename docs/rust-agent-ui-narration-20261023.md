# Narration 载体与 fallback

此阶段对照 ClientKit `TranscriptMetalActivity.swift` 的 `narrationCarrier`、
`runningFrameNarrationNodes`、`runningNodes` 与 `settledTimeline`，以及当前 journal projector。
仍固定公开 TekesUI `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`；不修改上游或引入 Swift。

## 公开业务契约

共享 presentation 新增 `TranscriptNarrationPolicy`，输入是捕获的 `TranscriptNarrationItem`、
`TranscriptNarrationFallback` 与 `TranscriptNarrationTimeline`，输出为 Item、GroupFallback 或 Empty。
候选必须提供真实可见正文节点的 text 列表、item/frame 身份和明确 canonical-narration 角色；
不要把 reasoning、工具结果、chrome 或原始 JSON 当旁白节点。

载体按每个节点的扩展字素数之和、canonical-narration 角色、身份排序；不能按字节、scalar 或
先拼接后计数。运行时间线中，若 canonical 节点文本仍是累计 group 文本的严格字符前缀，继续保留 group；
相等或不同内容立即使用 canonical。普通已结算 Activity 则在 canonical 节点存在时直接采用 canonical。
BetweenInputs 虽是历史，也按 ClientKit 的 runningNodes 分段路径选择。没有 canonical 节点时保留真实 group。

比较使用 Unicode 规范等价和完整扩展字素边界，原始文本不改写。依赖 `unicode-normalization =0.1.25`
与既有 segmentation；不是重写 UI 文本引擎。[Swift 官方字符串语义](https://docs.swift.org/swift-book/LanguageGuide/StringsAndCharacters.html)
说明 canonical equality/prefix。规范等价的重复 item token 或歧义 frame 拼写会拒绝，不合并来源身份。
该 API 不授予 session/history/提交权限，也不保存独立 Run 状态。

## 原组件组合

TranscriptContent、RunPort 与 ReadOnlyTranscript 共用原 planner。它从原 OutputMessage 的正文 part facts
取得当前 prose renderer 的可见 text，只有具备 scope/frame 来源且无 failure、附件或不透明内容的整帧才选载体。
原 canonical DTO、复制正文、reasoning 与工具来源不改写；不能把非最终 assistant 改成新的 canonical kind。

未选正文进入原 hidden_rows，原 Message 控制器、part ID 和 SDK 文本对象保留；reasoning/tool fragment
独立处理。选中的正文放到最早旁白位置，共享 sink 的 reasoning 先于正文，正文先于 call fragment。
隐藏的重复正文不再制造虚假连续工具边界。宿主不实现另一套 carrier 或滚动状态机。

当前 ClientKit journal producer 刻意不在 group lane 复制旁白，仅提供所有权/顺序，Root 当前也不补造 group fallback。
独立 fallback 的规则由公开 policy/第二 consumer 验证，不能把它描述为当前真实三端交付了独立 group 节点通道。
富内容节点尚未对齐时，整个相关帧保留各原正文，不用 JSON 长度猜可见内容；#25 相关能力与最终配对布局仍开放。

## 验证与剩余范围

程序、来源、差异、命令及收据保存在 `.runtime-fixtures/narration-carrier-20261023/`，最终范围见同名 JSON。
共享规则用例覆盖完整度/角色/身份、逐节点字符数、canonical Unicode/字素前缀、live/settled、空节点与错误身份；
第二 consumer/宿主组件用例覆盖两个独立载体的选择、reasoning 与正文的相对顺序、原控件恢复和 Composer preedit。
这些多记录捕获 fixture 不代表真实协议产生了该交叠状态。

**多记录载体切换时的稳定 frame 阅读锚点尚未验收**；原文本 owner 恢复、普通追加或分页锚点不能代替它。
旧 embedded tools 仍为消息级 fragment，跨追加输入的逐 call 分区和完整同内容对照仍需补验。
当前真实 producer 的单 canonical-per-frame 路径与上述 fixture 结果分开记录。
完整富内容/配对布局、正式同/跨进程 ProcessContainer #48、严格嵌套阅读 #53、稳定 IME/VoiceOver 仍未完成。
PR #10 保持 draft，严格失败门禁保留，不合并或打完成 tag。

## 下一步的 frame 锚点边界

已核对精确 SDK 的 `composition/content_reading.rs`：文档项目是 caller-owned `Vec<ComponentId>`，
不存在本轮可直接消费的 body-ID→逻辑-frame alias；失去已挂载 item 时执行 successor/predecessor fallback。
因此多记录动态切换不能仅凭新 body 控件恢复就声称阅读位置已保留。
后续应由 AgentKit 提供真实稳定的 NarrationFrame 根控件及其当前正文范围，原 OutputMessage 控制器仍分别保留，
阅读文档登记 frame 根，复制/来源定位仍绑定实际 canonical item。不得把 frame key 冒充 session pointer、
复用无来源关联的消息控制器或在宿主计算 offset。这是业务组合工作，不是另开 SDK 基础状态机的理由。

## 本轮当前程序结果

冻结 UI SHA-256：`0c3219f41cde7be2278db39e5e1f15a0050c75f1f7b816d458bae30f5e724506`。根 **308 通过**、UI **604 通过/1 原有忽略**，
两套严格 all-target Clippy 通过。根与 UI lock 均精确加入 unicode-normalization 0.1.25，SDK 版本不变。
三端真实 harness 配本地合成 provider 的协议/离屏 GPU 路径通过，原始 complete/rich/native false flags 均保留。
Native Kernel 为 0.9.3 UI-only。

原生 prompt profile **失败**：phase 0 窗口始终 inactive，前台轨迹只有 Codex；Close 时原完成断言报 0≠13。
保留原始日志、程序与前台轨迹，没有自动激活或重跑。其余原生 profile 和系统软件拼音未运行。
此前 6a3a49e7 的原生/IME 结果只属于历史程序，不转移到本轮。
`bf179f3` 的两次基础 CI 成功、两次 UI 在 #53 严格 nested-detail 步骤失败（官方 job API），门禁未改变。
本阶段没有新建 target，也未删除失败验收程序或共享缓存。
