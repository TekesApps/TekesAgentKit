# 最终 TekesUI 候选：AgentKit 独立接收结论

2026-10-19。结论：**可提升为新的显式开发候选，不可作为正式发布 pin；#15–#21 均尚不满足全部关闭条件。** 本轮没有覆盖 primary 的旧冻结候选或 WIP，没有提交、推送、tag、发布或修改 Issue。

## 输入与保护

- 新公共候选：`03721866a362b1d31261ebbbc86140eb07e9e23320871c325234a5212591213d`，188 个公共文件。
- 源码归档 SHA-256：`a220dfca80748f0b7d82f115697b59a156186852f5c433891e05cd7c65eb24d0`。
- 证据归档 SHA-256：`2731130ecc4705ded3a161f5c07e159f4c6c24ed6bd72115b5a2444dba9fc448`。
- 来源：TekesUI `design/reviews/agentkit-final-acceptance-20261019/` 的两个归档。先校验并安全解压到工作区外全新临时目录，未从活跃工作树 capture，未应用 `audit-only.patch`。
- 旧冻结 `e4c6cb6f725ff9df5ec4688927dd265d64dbbdb2380572d963c5b1017dde5bbc` 保持原样；primary 128 个原文件逐字节未变。所有本轮变更仅为本报告及其 JSON 收据。
- 候选相对 primary 的消费适配恰为 `ui/demo/src/native.rs`、`ui/demo/tests/text_viewport.rs`、`ui/tekesui-source.json`。新旧公共清单均为 188 个文件，其中 12 个文件不同；不等同“188 个文件都重写”。代码包不含 `native_qa.rs` 或 Swift。

## 独立复现结果

| 检查 | 本轮结果 |
|---|---|
| 归档及内容 | 两包 SHA 正确；545 个 payload、188 个公共输入全部通过 verify；证据清单 304 项全部匹配 |
| Cargo metadata | UI 的 15、公开 consumer 的 10、headless 的 8 个本地包全部解析在解压包内，没有 sibling checkout 依赖 |
| Rust 工具链 | 显式 `+1.95.0`；允许首次正常下载 registry crates |
| UI workspace | 28 项通过；含 AgentKit 第二 consumer 挂载及实际 controller/编辑/阅读用例 |
| 第二公开 API consumer | all-targets 131 项通过；测试支持转译文件均位于包内，不依赖 Gallery checkout |
| 默认 headless | 76 项通过 |
| demo build | 独立 native host 构建成功，未自动启动原生窗口 |
| 格式与 Clippy | AgentKit root/UI fmt check；UI、公开 consumer、headless 严格 all-targets Clippy 全通过 |
| 边界检查 | UI 原脚本通过；原 headless 脚本因归档无 `.git` 在 inventory 阶段失败，基于 payload 清单与完整 metadata 的相同依赖/无 Swift/无 sibling 断言通过；未将原脚本冒称为通过 |
| 实际阅读布局 | Earlier 连续两页均 0pt 误差；第二页包含宽度/文字缩放变化，选区与焦点保持 |
| 真实 GPU | 新构建 demo 通过 Metal/CoreText 离屏输出，图已检查；PNG SHA `819c3c0d9415b7c17eacec9b47825dfd19c897e817c241e6b4a396d48ac05fc5` 与交付证据一致 |
| 源码封闭性 | 全部构建结束再次 verify，545/188 输入不变；所有构建复用一个包外 CARGO_TARGET_DIR，未复制 target |

完整路径、日志哈希、62 条逐行结论见 [JSON 收据](tekesui-final-consumer-review-20261019.json)。本轮与上游测试计数分开，未将上游 workspace 测试算成本会话重跑。

## 逐 Issue 结论

以下“可提升”仅指该精确开发包及三处消费适配的技术接收建议；没有执行 primary 提升。所有 Issue 的公开不可变 pin 条件都仍缺失。

| Issue | 独立复验与证据判断 | 可提升开发候选 | 全部关闭条件 |
|---|---|---|---|
| #15 WindowWake | 通知/流式/取消/连接回归通过，native host 构建无持续业务 frame pump。已复核交付的双真实 NSWindow 定向投递、关闭代理和 idle 计数记录；旧双窗保持原版身份，平台/wake 未变 | 是 | 否：缺公开 pin；primary 未提升 |
| #16 前插锚点 | 实际 Earlier 两页、重排、A/B/选区/焦点及公共 consumer 前插测试通过；交付最终037原生两页同80 UTF-16文本及Y偏差0pt的坐标记录可追溯。实际点 Earlier 会转移键盘焦点，不据此声称点击后焦点仍在 Composer | 是 | 否：同上 |
| #17 输入/IME | disposition、修饰键、repeat、Tab/多行/提交隔离回归通过。物理 Enter/Space/Tab/Escape证据按原候选复用边界保留；037物理组字记录单列，不冒称所有键都在037重新实测 | 是 | 否：同上 |
| #18 受控事务 | revision/选区/undo/字素/晚到回填/A-B隔离测试通过；复核最终037物理组字期间 Composition 冲突、确认后 Revision 冲突及无REMOTE覆盖记录 | 是 | 否：同上 |
| #19 指针 | 完整转译及选择、捕获/次按钮/取消回归通过；复核旧原生 Shift/双/三击、出界拖选和未抬起时失焦链。工具链的 OS pressedMouseButtons=0，不算硬件鼠标按住体验 | 是 | 否：同上 |
| #20 文本视口 | Fill/Grow/readonly/clip/backing/活动组字几何回归通过；复核最终037物理候选跟随、实际 AppKit 命名测试剪贴板复制/拒绝写入、VoiceOver独立进入/当前行/下一行的人声结果 | 是 | 否：同上 |
| #21 类型化动作 | ID/source、顺序、drain、disabled、重挂和源目的地捕获用例通过；消费代码不轮询 activation 计数，原生 pointer/key/AX 保持各自版本证据 | 是 | 否：同上 |

实时读取 GitHub #15–#21，7 项仍 OPEN，正文共62条，与交付矩阵逐条一致。本次把55条技术/分层证据要求确认为开发接收范围已满足，7条公开 pin 条件为阻塞。机器收据保留上游原状态和接收方结论，不回写或重标旧证据。

## 原生、物理与设计证据边界

本会话独立重跑的是自动测试、构建、metadata 与离屏 GPU，**没有新开原生窗口、开启 VoiceOver、注入物理输入或操作通用剪贴板**。交付方原生/物理/人声证据已按源和二进制身份核查，属于已提供证据的接收，不是本会话现场重跑。

- 最终037 end-to-end 成功日志与第一次失焦准备不足的失败日志均保留；成功用显式聚焦的审计副本，不把 audit patch 混入可消费生产包。
- 最终037原生 Earlier、named AppKit copy、物理拼音及人声 VoiceOver分别留证。命名测试剪贴板不是跨应用通用剪贴板验收；系统设置和人声不由自动AX查询替代。
- `prior-native-provenance.json` 相对旧4a02的唯一公共变动为 TextSurface 几何；本次 primary e4→037有12项公共差异，是不同比较基线，不混淆。
- 已复核 G5阅读连续与目标隔离、G6输入/组字/可见几何、G7源码和可复现证据。代表性 Light/外接2×、API1×/2×及Dark事件不代表全主题/全组件 Gold7 完成。性能不测。

主要交付证据相对其证据包：

- `design/reviews/agentkit-final-acceptance-20261019/`：验收矩阵、end-to-end/anchor/copy/VO回执、版本复用边界和完整原始事件。
- `design/reviews/composing-viewport-fix-20261019/physical-native-result.json`：最终037活动组字/几何/受控事务。
- `design/reviews/held-pointer-cancel-20261019/native-result.json`：工具驱动Down→失焦→Up，明确硬件限制。
- `docs/design/p0-live-windows-20261019.md`、`docs/design/pointer-action-native-20261019.md`及后续专项记录：保留各自版本、尚缺事项和补验历程；最终状态以对应后续证据为准。

## 提升范围及后续

技术建议：若授权实际提升，仅从已验证归档复制三处消费适配及匹配188公共输入，保留旧 manifest/.deps 与原WIP校验，仍标记 `development-snapshot-not-release`、UI `0.0.0/publish=false`。不可从活跃 TekesUI 重新 capture，也不要覆盖 AgentKit 的其他业务源码。

本轮仅给出接收与提升建议，未执行上述覆盖、commit、push、tag、发布或 Issue 操作。正式发布前必须取得包含这些字节的公开不可变源码 pin，并按该 pin 复验；发送方在所有关闭条件实际满足后才能关闭原 Issue。当前七项均不能关闭，不以“功能通过”替代缺失的交付条件。
