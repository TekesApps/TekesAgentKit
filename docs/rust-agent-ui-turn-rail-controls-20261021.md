# RunView 回合轨道：实际组件与独立滚动

AgentKit 组合原 TekesUI `ScrollContent`、`CompoundButton` 和被动 tick 装饰，拥有回合身份、排列、选中与 follow-target 业务规则。库继续拥有按钮 arm/cancel、键盘/AX 激活、焦点、IME、命中、滚动、动画和绘制。没有隐藏按钮、替代编辑器或消费侧滚动补偿。

对应只读参考为 ClientKit `RunChrome`、`TranscriptIndicatorRailView` 与 `RunView.jumpToTurnFromIndicator`。保留 10pt 行、中间至多 70% 高度的独立轨道、窄 gutter 下的居中 tick、初次最后一项及两个行高的 follow comfort zone。颜色和交互 chrome 来自库；原 ScrollContent 在文档内预留的焦点环空间也保留，行占满库安排的文档宽度，不覆盖库的 inset。水平滚动关闭、竖直滚动启用并隐藏滚动条，与对应 View 一致。

## 公开接入

- `RunView::install_turn_rail(namespace)` / `with_turn_rail`：在原 Run 内挂载，命名空间必须与原组件及 Transcript 行池分离，不替换 Composer。
- `turn_rail_supported()`：默认公开 pin 不具备 Exact/Press 组合能力，返回 false；安装在修改前明确报 unsupported。
- `set_rail_navigation_gate(TurnRailNavigationGate)`：宿主分别声明能执行 TurnStart 和 Latest；默认为关闭。关闭、再打开同一 gate 会退休原 gesture lifetime。
- `RunViewIntent::TurnNavigation`：原按钮在事件发生时捕获不可变来源，经 `take_intents` 返回。宿主须先 `claim_turn_navigation`，立即调用对应原 SDK 方法，再请求 render。
- `TurnRail::item_ids / viewport_id / document_revision`：供独立库阅读文档使用。成员改变保留同回合的控件 ID，新增 ID 不复用已删除成员的身份。
- `update_rail_follow_target(following_latest)`：输入来自原 Transcript 的库阅读状态；业务选择最后回合或第一可见回合。
- `next_reveal / claim_reveal / finish_reveal`：首次 Nearest 揭示最后 tick，follow 变化且离开 comfort zone 时 Centered 揭示；宿主使用原 renderer、稳定 item 和原阅读版本。accepted 即使 changed=false 也要 finish、重新 render。失败暂停，仅新输入或显式 retry 恢复；同一已完成 follow 不反复覆盖手动滚动。

不要在 `ComponentDemo::sync` 更新来源后才消费旧帧导航。demo 的本地命令在该次刷新前同步交给宿主，后续 redraw 再刷新；发送/修改类事件的组字屏障没有放宽。只允许当前正在组字的 Run 中明确标记为不转移焦点的轨道动作，不能因此切换会话或借用另一 pane。

canonical 更新应在同一 UI turn 内预先检查原 Run 计划、原编辑器和两个阅读 owner 的 expected token。应用原 `RunViewUpdate` 后、下一次 render 前，成员/版本已可通过 rail 公共查询取得；宿主同步 advance 原 rail reading attachment。中间不得处理事件、await、重挂 owner 或发布 native 几何。SDK callback 不是任意业务失败的回滚事务；无法满足这些前置条件的宿主不得声称已完成双 owner 原子更新。已有帧的阅读记录与几何恢复仍由库维护。

导航捕获保留真实 scope、publication、epoch、原行成员、Run generation 和 mount/pane lifetime。卸载重挂、折叠重开、同版本权限 ABA、来源发布和 disabled gate 均拒绝旧请求；同一 gesture 的 clones 共享一次认领。

## 固定候选与尚未支持

默认 pin 仍为公开 `8cf0840790334ba224afc0595897c74231c712e6`，没有提升冻结或发布稳定 UI API。

本轮显式隔离消费 #29 候选 `c7797b1e1b5a26b079ad7d1b2f8b45b89d054b730cb915ec554077132e8db0b3`，489 文件；在未修改的 10b5 输入上增量 11 路径，归档 SHA256 `e926b626ac58f92ffbd8c56f176a70d5de96560af4479db4a2bc7fe0592a165f`。runner 只在隔离 AgentKit 快照启用原 Exact、Press、preserving_focus API；八个 UI 包均取自该解压闭包，不读取 producer 活跃源码。

- [#32](https://github.com/TekesApps/TekesUI/issues/32)：c779 仍无 item Start。普通回合执行保持关闭；Centered 只用于技术探针，不能算产品对齐。
- [#33](https://github.com/TekesApps/TekesUI/issues/33)：原库焦点层提升只识别直接 ButtonContent/CompoundButton，业务包装的原紧凑 action 无公开转发接口。AgentKit 不补焦点环、不改兄弟绘制顺序；键盘激活通过不代表焦点环视觉通过。
- 与 ClientKit `recordTurnJump` / `takePendingTurnReveal` 对应的宿主导航历史回调仍待对接，不能把本地滚动命令等同于宿主导航闭环。
- hover 连续 ramp、邻项响应与 preview 尚未完成。原生滚轮、物理 IME、VoiceOver、真实三端混合分页/阅读与配对产品页视觉均须另验。

同名 JSON 记录本轮实际检查与原生证据；这些不取代完整交付审计。完整目标继续有效，尚不能据此合并或打完成 tag。

## 隔离复验

从本仓库根目录运行，`--source` 与 manifest 必须来自同一精确归档，`--workdir` 使用新目录；缓存必须在源码闭包外：

```sh
python3 scripts/test-reading-candidate.py \
  --source /absolute/extracted/source \
  --manifest /absolute/extracted/portable-manifest.json \
  --probe scripts/probes/agentkit-turn-rail-candidate.rs \
  --append-run-probe --turn-rail --item-navigation-native \
  --workdir /absolute/new-receiver-check \
  --target-dir /absolute/shared-build-cache
```

命令只运行 CLI 检查并构建原生诊断，不自动打开窗口。确认自己的所有重构建已结束、桌面可用后，再将该次构建的 `tekes-agent-ui-demo` 保存为独立二进制，运行 `--components --transcript-content --item-navigation-selftest`。窗口检查使用本地 fixture、原 AppKit 合成 preedit 和实际原按钮点击，不调用模型。通过与失败尝试都需要按精确快照、二进制哈希和事件日志记录。

## 本轮证据范围

默认公开路径整包348项、c779 隔离路径整包362项及两条全目标 strict Clippy 通过；候选包含14项原 Run 事务/实际 rail 检查，第二公开 consumer35项。SDK controls/text 合计437项、独立SDK consumer213项，紧凑API专项7项属于其子集，不重复累加；三包 strict Clippy/native example/解压内依赖闭包通过。默认199文件与旧10b5/c779输入均重验未改。

最终带原生事件记录的同一二进制连续两次完成实际 Latest rail 点击：原 Editor、focus、输入 baseline 和合成 AppKit preedit 保留，Down/Up只派发一次，无提交。截图中用户气泡及外置 footnote 靠右，机器正文和 footnote 靠左；这只是当前fixture的显示证据，不是配对 ClientKit 产品页验收。普通回合的 centered 仍只是技术诊断。

失败也保留：首次探针在 render 前用完整行中心，违反当前可见命中前提，已改为成功 render 后的库 visible rect；另一早期窗口在末尾遇到publication5→6，缺少原生事件来源，原因未确认，不能用最终两次通过反推其已解决。后者仍属于验收限制，尚不建议提升冻结。

后续[#32 Start候选](rust-agent-ui-turn-start-20261021.md)另有独立输入与组件验证；本页c779收据和原生范围保持不变。
