# 输出脚注合并为一个阅读项（2026-10-22，实验状态）

只读对照当前 ClientKit `TranscriptTurnFootnote.rawOutputNodes`、`FeedbackNoteEditorView`，
MetalTextKit `MetalVirtualLayout.appendAside` 的 output lane，以及 Tekes
`ClientAgentHostAdapters` 的 Run 接入。来源文件哈希保留在机器记录，不复制 Swift 或要求 Swift 工具链。
Source 的 output lane 为左对齐单行，顺序为复制、评价/备注、失败信息、完成时间等；
live owner 仍只显示原 running line。布局与显示规则属于 AgentKit，控件/样式属于 TekesUI。

## 组合与公开 API

此前 `TurnFootnote`、`TurnFeedback` 是两个纵向文档行。现在一个 `TurnFootnote` 阅读项
拥有原 `TurnFeedback` Content 子控件，复制、评价/备注、状态、时间等按同一水平带排列，
原 SDK `layout_measured_children` / Stack 负责放置、裁剪与垂直居中。
SDK 仍拥有按钮、编辑器、焦点/IME、AX、文字与绘制；没有新的执行、反馈或滚动状态机。

- `row_keys()` / `TranscriptContentUpdate::item_ids()` 只列物理阅读项，评价不再重复占一项。
  `TurnFeedback(turn)` 保留为兼容查询：`row_id()` 返回原嵌套 control root；它不是另一个阅读项。
  收集反馈动作应使用 `feedback_controls()` / `feedback_mut()`，原 callback root ID 保持可用。
- `TurnFootnote::feedback()` / `feedback_mut()` 返回当前挂载的原控件。每个输出 aside 占用
  两个完整 namespace（footer、feedback），其他行一个；预测、预检、提交使用同一计数，
  过小范围会在改树前拒绝。宿主需为该实验 API 调整原预留范围，不能缓存旧的逐行分配假设。
- canonical response 修正时保留原 aside/feedback root，退役叶子按钮和旧 pending gesture；
  root ID 不是反馈授权。晚完成仍通过原 gesture/target/binding 检查，不覆盖新回复。
  目标消失或隐藏会清除其交互授权；A→B→A 不复用旧按钮 ID。
- 备注 editor 正在组字时，隐藏、替换回复或关闭 feedback 仍在改树前拒绝。
  目标相同时继续保留原 editor、owner、marked state 与原反馈 controller；没有另写备注状态机。
- 上游 #49 交互 Popover 接收门禁仍未通过。打开备注时沿用原完整纵向表单；这是明确保留的
  诊断 fallback，不以自制 overlay 冒充 Source popover。关闭后的正常 aside 才是本轮单行布局。

## 验证

最终程序 `.runtime-fixtures/AgentKitOutputAside20261022.app`，SHA256
`0f26c1ff65602fee363f1aa39ed93d9be15e444f128a5cf7d55f5ef1fe615c8d`，
程序、9个改动 Rust 文件和只读来源哈希逐项核验。

- UI workspace 517项通过，1项既有 #49 门禁 ignored；fmt、strict all-target Clippy、demo build、
  边界和实际公开 source closure 通过。闭包复用未改变 manifest/lock 的已解析 metadata，再核验
  当前不可变源码8包/241文件；未宣称重新解析了新的 SDK。
- 独立 public API consumer 使用 CoreText，在4主题、3字号比例、320/770/1100宽度核对原按钮
  顺序、左端与垂直居中、单一阅读项及原反馈实例保留。这是几何/生命周期证据，不是全部可见性验收。
- 回归覆盖 namespace 预检/原计划、组字期间拒绝替换、旧反馈晚完成/旧按钮失效、隐藏再出现不
  复用旧叶子、原 pressed 按钮在主题/状态更新后仍正常激活。更新了原纵向布局/独立反馈行断言；
  业务授权、无重复提交和组字断言没有放宽。初始失败/编译诊断保留。
- 同一程序真实 Pi1.3.0、DSH0.2.1-rc.2、Native0.9.3 的原生反馈闭环全部通过，含实际单行
  geometry 断言、无评分时隐藏备注、保存/重开中文备注、同评分删除、显式重新读取、A/B草稿、
  原 Composer owner/文字/焦点及组字屏障，无 prompt 提交。opaque baseline 因确认受控草稿版本
  会推进 epoch，沿用原收据的 false，不篡改成完整 opaque 保留；独立 consumer另验原始 baseline。
- 同一程序 owned fixture 主阅读20→44、只读子历史38→75通过：前插/刷新锚点、原分页/
  Start/latest、A/B及重连阅读、主/子编辑器与合成组字、跨 pane 复制保持；不是实际三端原生分页。
- 同一程序正常 app 系统拼音一次完整通过，SCIM 输入源未切换，原 marked/确认与单次提交路径
  保留。软件 IME 不等于实体键盘、候选窗、VoiceOver 或长期稳定性。
- 三端公开数据到离屏 GPU 各自通过；Native 为明确 UI-only，不证明完整 lifecycle/archive/child。

## 实际限制与后续

离屏截图已检查：正常 band 确为左对齐单行，但窄列使用原文字按钮时，右侧时间/附件可能被
挤出 clip。**窄宽全可见、长备注 Source 40%上限与截断、紧凑图标和成对视觉仍未完成**。
不能仅凭 frame 顺序测试或本轮 geometry 收据宣布完整 Source 排版验收。
图标/被动 Content 与 renderer seam 仍跟踪 #47；公开版后续消费必须重新确认实际能力。

本轮编译与所有收据固定于 TekesUI `09b4bab44b6dfa65487250ea816b610b9df59095`，
清单 SHA256 `44b3174022584d3c099a45b86892d17dbf948905476c0f038be104df12aefe46`。
验收期间发现公开 main 新合入 `3bef66754a256e7723887de7d33518f5122805ad`（toolbar/split等），
尚未审查或消费；下一阶段核对完整依赖闭包/迁移后精确升级，不把本程序证据归到新版本。
ProcessContainer #48 仍 OPEN；本轮只证明同进程原 Content。整体目标未完成，PR #10 仍 draft，
不合并、不打 tag。

原始日志、所有失败、成功收据、前台轨迹、截图与程序证据在
`.runtime-fixtures/output-aside-row-20261022/`；摘要见[机器记录](rust-agent-ui-output-aside-row-20261022.json)。
复用现有 UI/root target，未新建 target、未删除缓存，实际释放空间0；保留源码/WIP、固定依赖与验收程序。
