# 回合预览事实与被动 hover 卡片边界

当前 ClientKit `TranscriptIndicatorRailView` 的预览是输入/输出两个层级：输入最多2行，输出最多4行；输出空但未settled时显示Running，settled空输出不占位。卡片在track右侧，按tick和viewport定位，hitTest不接管任何事件；离开/Escape收起。Tekes宿主的`ClientAgentHostAdapters`只提供导航/history能力，没有替代预览控件。

本阶段补齐正式turn target所需的预览事实和只读来源查询，**尚未显示hover卡片**。公开SDK的原owner帮助只接受String，现有PopoverView又自带Dismiss几何/交互，不等价于该被动卡片。已提 [TekesUI #46](https://github.com/TekesApps/TekesUI/issues/46) 请求原Content槽、锚定、非交互overlay、失活/裁剪/卸载/版本退休及保持IME的关闭契约；行数/省略沿用#31，Panel表面沿用#45。不把输入/输出拼成普通tooltip，不在AgentKit复制通用overlay或滚动引擎。

## 公共数据契约

`TranscriptTurnTarget::preview()` 返回 `TranscriptTurnPreview`，是与该target同一scope/publication/epoch/原Content行关联的不可变显示快照，不是UI组件或操作许可。

规则明确对照ClientKit的 `SessionEndpoint/AgentKit/AgentKitTranscriptView` → `TranscriptTurnSummary`：

- input保留原turn.input；不读当前Composer草稿。
- 同turn的assistant正文按原Snapshot.items顺序用换行连接，再取前240个extended grapheme clusters。与该桥接路径相同，不把reasoning/tool/error拼进output；不是重新运行各harness原生projector。已有isVisible/正文语义仍按桥接事实保留，不另猜内容。
- has_final_answer直接取同turn assistant的isFinal；is_settled按规范化AgentKit桥接规则`status != running`，不依据输出文字或结束时间推断。
- `output_text(running_label)`只在空输出且未settled时给调用方的本地化Running文案；settled空输出返回None。换行空项保留，不擅自trim。

这只是当前公共AgentKit投影对应的plain preview事实；ClientKit的产品级host payload注册过滤/最终富内容仍需对应显示契约，不在此猜标签、解析模型私有载荷或提前宣称完整预览呈现。

`TurnRail::preview_target(turn_id)`取得原rail项捕获的target；`RunView::turn_preview(&target)`验证当前挂载/呈现、原Transcript owner、publication/epoch/行与内容以及当前布局来源。允许在原Composer组字期间只读查询，不认领导航gesture、不调用host history、不改focus/selection。旧publication、不同owner、正文/status变化及折叠会拒绝旧target。数据验证不能代替未来SDK的anchor/mount/overlay权限；宿主不得把旧DTO移到新会话或在未经SDK验证的anchor上显示卡片。

第二公开consumer暴露同一查询函数，由实际RunView/原SDK组字测试调用。预览内容与布局必须继续由AgentKit组件组合，宿主只提供窗口/导航能力，不能要求宿主另写Agent预览状态机。

## 依赖与验证

新增精确`unicode-segmentation = 1.13.3`直接依赖，仅用于ClientKit的240字素业务字符串规则。该包已在原UI lock内；Cargo.lock只增加components包的一条依赖边，没有新包或版本更新。没有按Unicode标量/字节截断，也没有实现文字排版。边界检查显式允许并固定该依赖。

TekesUI仍为精确公开`09b4bab44b6dfa65487250ea816b610b9df59095`，实际八包241项输入，清单SHA `44b3174022584d3c099a45b86892d17dbf948905476c0f038be104df12aefe46`，通过实际Cargo metadata复核；无本地SDK覆盖。

450项UI workspace测试、第二consumer、strict all-target Clippy、fmt、边界与实际消费输入检查及native build通过。新增/扩展覆盖：assistant顺序与换行、空/运行/结束状态、排除reasoning/tools/error、emoji ZWJ/组合音标240字素边界、跨消息CRLF、原rail来源、组字保持、旧publication和折叠拒绝。首个prefix测试夹具忘记设置可导航输入，按既有blank-turn过滤没有target而失败；补正确输入后保留全部prefix断言，不改业务过滤。

冻结程序SHA256 `a85d42f069ece5f009cf7c9d09b6e5807867369be1b577813f2ff9d3c4c18134`，保留于 `.runtime-fixtures/AgentKitTurnPreviewCheck.app`。原生`--components --transcript-content --reading-native-selftest`通过20→44项分页、前插锚点、A/B阅读与草稿、重连、Start/Latest和中文marked；在普通回合点击前额外读取原rail预览，原editor focus/baseline与host history计数保持。收据明确`previewFactsDuringPreedit=true`且`hoverPreviewVisible=false`。这是owned fixture/synthetic preedit/wheel，不是卡片视觉、实体输入/VoiceOver或本轮真实三端重新验收。

证据在 `.runtime-fixtures/turn-preview-20261022/`：程序与源/reference哈希、实际metadata/inputs、测试/Clippy/build日志和原生收据。没有修改ClientKit/Tekes/Kernel/TekesUI。复用既有target，debug0、incremental关闭，无新target；源码/WIP、固定依赖、日志与程序保留，本阶段删除字节0。

PR #10继续保持实验草稿。完整rail hover/ramp/card、同名Views配对视觉及既有其它差异仍未完成，不据本次数据路径合并或打稳定tag。
