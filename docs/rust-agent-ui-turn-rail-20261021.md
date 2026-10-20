# 回合 rail：真实目标与布局输入（实验）

这一轮补齐正式 Transcript / RunView 的回合目标和 rail 布局输入。它们是后续真实 rail 组件的基础，不是已挂载的可交互 rail，也不宣称已经有跳转、hover 预览或完整 ClientKit 视觉一致性。

## 公开输入

`TranscriptContent::turn_targets()` 只读取已安装的 canonical snapshot 和原内容树：按 canonical turns 顺序返回非空白 input 回合；每项包含原 scope、publication、connection epoch、turn ID、input/status，以及实际保留的 input/output/tool/unplaced/footnote/feedback 行 ID。会话提示、诊断和孤立记录不被猜测归入最近回合；空白 input 与 ClientKit rail 一样不显示刻度，但不删除它们的 Transcript 内容。

`TranscriptTurnTarget::is_visible()` 是实际安排后的行与真实 clip 的交集，不以加载范围、末项或运行状态推断。新 publication 安装后、尚未 arrange 时返回 None；`RunView::turn_targets()` 在 pane 折叠时也返回 None。其余情况是明确的 Some(true/false)。没有另存一套阅读状态或反算滚动坐标。

捕获后，在原 `TranscriptContent::validate_turn_target` / `RunView::validate_turn_target` 再检查原组件 owner、scope、publication、epoch 和行成员。独立窗口即使复用相同数字 ID 也不能冒用捕获；重连、裁剪和更新后必须重新取得目标。RunView 另在折叠和 Composer 组字期间拒绝导航。此校验不授予 endpoint 权限，也不产生滚动副作用。

`RunViewGeometry::turn_rail` 是当前 pane 下**计划的 rail 槽位**，尚不是实际挂载子树的渲染证明；与其余 geometry 一样需检查 pane 和 publication/epoch。`TurnRailGeometry` 对齐只读 ClientKit 的 RunChrome 和 TranscriptIndicatorRailView：

- rail 占 source leading gutter，使用 leading fraction；没有显式 fraction 时取默认 reading 几何，与正文 FullWidth 分支独立。
- Regular 起始净空和 Loose 刻度前距来自 TekesUI token；正常宽度覆盖最长刻度，窄 gutter 时居中并裁剪长度。
- 每回合10点，track 在 pane 中居中，最多占70%高度；文档高度保留完整回合数，长轨道的滚动归库。
- 刻度厚度来自 FocusBar token。6/26和10/70%是对应 View 的业务布局规则，不是另一套颜色、字体或圆角皮肤。

## 基础接口与仍未完成项

- [TekesUI #28](https://github.com/TekesApps/TekesUI/issues/28)：按稳定 item reveal，保留 Composer 焦点/组字，并支持 nearest/centered、可见边距及 library motion。补充 #22 的显式导航入口。公开8cf的 ScrollArea 接受文档 Rect，但 Content ID 到祖先滚动的映射入口是私有的；不能用 focus 替代跳转。
- [TekesUI #29](https://github.com/TekesApps/TekesUI/issues/29)：明确 opt-in 的精确紧凑 action frame、装饰 slot 与可选 press 激活；库持有命中、capture、键盘、Cancel、focus ring 和 AX。公开8cf的 CompoundButton 固定 Body 最小高度，不能把它的重叠热区当作10点回合轨道已完成。

AgentKit 仍负责实际 rail 组件、连续 hover 邻域、预览文案与内容分层、visible/follow-target 输入、同目标只跟随一次、真实目标导航策略和与 RunChrome 的完整组合；不会把这些业务工作交给 TekesUI。动画采样、滚动钳制和焦点生命周期由库执行。

本轮没有改 ClientKit、Tekes、Kernel、TekesUI 或默认精确 pin。测试、第二公开 consumer、候选阅读探针及原生回归的最终结果见同名 JSON 收据。原生回归仍是 fixture，不能代表真实三端 rail 导航、物理 IME、VoiceOver 或配对视觉验收。完整目标继续保持未完成；不据此合并或打 tag。
