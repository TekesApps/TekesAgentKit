# RunView 公开组合与宿主边界（2026-10-20）

`RunView` 是 AgentKit 的可独立挂载组件，组合已有 Composer、Indicators、TranscriptPaging、ActionableCard 和可安装的 TranscriptContent。它不是 RunSurface / ComponentDemo 的改名或包装；这两个仍是测试宿主。主会话和可写任务、独立子代理、SideChat 的实验宿主现用同一个 RunView 更新和意图出口。

这是可接入的实验组合，尚未通过完整 Run/Transcript 验收。只读 Native 有界子历史现已接入独立 [ReadOnlyTranscript](rust-agent-ui-readonly-child-20261020.md)，不假造 RunController 或可发送 Composer；原 RunView 收据保留此前验证范围。阅读/分页锚点、富内容、真实中文 IME 和完整三端原生 UX 也未完成。

## 依赖与来源

AgentKit 最新已发布基线仍为 v0.3.8；本 API 尚未发布。TekesUI 固定公开 `8cf0840790334ba224afc0595897c74231c712e6`，八包199输入 SHA `f0af230a1efe97334b90eb193f77ca0d30dec43ef09e487ad6948c38a938257f`；没有跟随兄弟 worktree 或改动 Kernel/TekesUI。

只读对照 ClientKit `c289bc9d33edcd83a782ac17927a8c65a027eccd` 的 Composer/Run/RunView.swift 及 ClientEmbeddedRunView：同一 Run 组合复用于 panel，宿主提供环境、导航与生命周期。Tekes `8858ccd9d46590852ec7a0949aee289fc6798d70` 保持宿主适配边界。Tekes 的 ClientAgentHostAdapters / ClientAgentPanelHostAdapters 继续仅接环境、导航、系统能力与 shared panel 控件；不移植宿主状态机。本轮没有 Swift 源码或 Swift 构建。

## 状态输入与构造

```rust,ignore
let mut view = RunView::standard(100, 104..1104, original_scope.clone())?;
view.update(&run_snapshot, affordances, gate, page_facts, actionable_controls)?;
let root = view.ids().root;
let host = view.into_host()?;
```

`standard` 占用四个连续 namespace：控件、分页、问答、内容根；最后的范围由宿主独占授予消息行。`new` 接受明确 RunViewIds 和可预先配置的 Composer（包括 context）；`with_transcript` / `install_transcript` 安装一次内容树。隐藏 context、未来行池和预留控件也参与 namespace 重叠检查；内容树不能被另一个来源替换。宿主可通过 append_accessory 接入刷新等产品能力控件，须遵循整个窗口的 ID 预留协议。

每个 RunView 固定一个 original RunScope 和一份 controller authority。update 接受来自同一 RunSnapshot 的 revision/connection_epoch/selection/run/draft/transcript；不分别重读 Runtime 拼接。上一帧仅作为渲染基线，用于旧版/同版冲突检查，不管理会话或提交状态。分页的 supported/source_ready/read 来自原绑定的 HistoryPageSnapshot，has_more/stale 必须匹配该帧；后来采样的 epoch 不匹配时只能降低权限。

更新先验证来源、版本、内容树、问答 DTO 及分页事实，再采用新草稿。实验宿主捕获分页请求后仅在末尾完整同步中发布 loading，避免额外读取新快照并将其与旧 RunView 内容拼接。重连、提交、队列、历史合并、草稿和 pointer authority 继续归原 presentation/runtime。`update` 同步已有组件；不启动网络、订阅、计时器或重试。

## 事件回调、焦点与 IME

每次 ContentHost event/AX 交付后立即 drain_actions，并调用 RunView.take_intents，随后用共享层的新 snapshot 更新。返回类型为：

| RunViewIntent | 宿主行为 |
|---|---|
| Composer | Edit/Send/Cancel/入口/context 交原 RunBinding；Send 仍先共享 admission，再异步 preparation/执行 |
| LoadEarlier | 在原绑定捕获 HistoryPageRequest，再交执行器；不等导航后重查选中来源 |
| Actionable | 原绑定 begin_actionable_response；回应回到原卡片，Unknown 不自动重放 |
| TurnAction | 原 TurnActionDispatcher 捕获同一原 scope / turn / boundary；晚导航按原宿主 token 处理 |

文字、键盘、marked range、原生输入服务、选区和 undo 由 TekesUI 持有。宿主在投递会改变焦点的 pointer/AX 前，必须检查 `allows_focus_target(target)`；组字期间只允许当前 editor。窗口内其它弹层或 pane 若在组字，宿主还须应用窗口级的原 editor 屏障。不要等焦点已转移后才检查 marked 状态；原生窗口可能已经结束该 composition。普通文字/按键继续原样交 TekesUI，Enter 不由 RunView 转成 Send。

实验原生宿主保留原窗口级屏障，第二公开 consumer 可用上述公开许可接入。组件测试用合成 Preedit/Commit 验证许可、未发送和提交后只产生 Edit；它不代表物理中文输入法通过。

## 排版、主题与滚动

RunView 使用 TekesUI StackContent / ScrollContent，viewport 使用 Composer 等固定内容之外的剩余高度。Composer 必须留在窗口内。用户附件 → 气泡 → 外置 footnote 靠右；机器内容和 aside 靠左。颜色、字体、圆角、间距和渲染完全沿用 UI 库默认值及宿主主题，显式升级 pin 后整体体现变化。

ScrollContent 当前只提供通用 viewport。[TekesUI #22](https://github.com/TekesApps/TekesUI/issues/22) 仍缺混合子项的稳定阅读事务；[#25](https://github.com/TekesApps/TekesUI/issues/25) 仍缺可选中富内容与原生映射。RunView 不缓存自有阅读状态、不补偿像素，也不把保留控件 ID 说成阅读锚点通过。内容能力通过 transcript().capabilities() 读取。

## 验证

分层证据见[相邻 JSON](rust-agent-ui-run-view-20261020.json)。UI workspace 228 项、脚本 18 项测试及严格 UI Clippy/fmt 通过；headless 行为未改，没有重复全量 root tests。公开组合测试覆盖真实子组件挂载、原 destination Edit、scope/epoch/同版冲突、非法正文/辅助事实在采用草稿前拒绝、隐藏 namespace、合成组字许可；第二 consumer 只使用公共包构造并同步 RunView。

真实 Native/Pi/DSH 的隔离长历史、正文＋附件与纯附件、重连、分页及 GPU 路径通过同一 Root 组合和意图路由；provider 为本地合成服务。原生 fixture 单独验证 Send/Cancel/草稿/重连、任务/子代理/SideChat 及树中 footer 指针入口；不扩展为真实三端完整原生体验。历史 probe 中的 completeRunView/completeTranscript/keyedReadingAcceptance 继续为 false。
