# 公开 pin 固定与逐 Issue 独立复验（2026-10-19）

结论：**已将 primary 的 TekesUI 依赖固定到公开 Git commit，独立消费复验通过；按 #15–#21 各自原条件，七项均可关闭。** 上游整体CI仍有一个独立Gallery golden失败，不把它记作通过，也不将本结论扩大为TekesUI全组件、全主题、完整Gold7或稳定版验收。原Issue关闭由获授权的发送方执行；本会话未操作Issue。

## 精确输入与解析

- 公开仓库：`https://github.com/TekesApps/TekesUI.git`。
- 唯一固定rev：`431093786de47b48cd566d0d9c0eafa088b10d68`；不依赖分支名或移动tag。
- 与已验收037的188个文件逐字节相同，聚合SHA-256：`03721866a362b1d31261ebbbc86140eb07e9e23320871c325234a5212591213d`。匿名GitHub commit查询、临时bare fetch/逐blob比对通过：changed/missing/unsupported/consumed-crate-added均为0。
- `ui/Cargo.toml` 的6个直接依赖已换为git+完整rev；Cargo.lock解析8个TekesUI包（含motion/design-export）为该同一Git来源。比较旧lock：仅这8项新增source，其他依赖版本、checksum与内容未变。
- 实际Cargo来源根：`/Users/frankqin/.cargo/git/checkouts/tekesui-d66ba5efbf6c8866/4310937`。8个包的具体manifest路径及source记录在JSON收据，全部188个实际解析输入再次校验通过。没有读取旧.deps、活跃兄弟工作树或手工复制的WIP作为构建依赖。
- 已应用此前独立验收的 `native.rs`、`text_viewport.rs` 两处消费适配；继续保留本会话新增的两项故障回归。不fork公共控件、文本算法或新增业务状态机。
- UI继续 `0.0.0` / `publish=false`。此pin提供公开不可变源码，不表示AgentKit UI已发布。AgentKit本地修改未提交/推送，未打tag。

## 旧冻结与保护

旧e4 manifest、Cargo.toml、lockfile、两处适配、边界/准备脚本及188依赖的副本保留在 `/Users/frankqin/.codex/artifacts/agentkit-pin-backups/` 下本轮专属目录，精确路径见JSON收据。原 `ui/.deps/tekesui` 仍保持e4的188字节不变，当前Cargo不引用它。封存037源码/证据包未改写。

新source manifest明确 `published-git-revision`、repository、rev和037文件清单；guard要求Git来源/rev一致，拒绝混用path或移动revision；prepare/capture在该模式拒绝运行。未修改TekesUI checkout、Kernel、其他AgentKit业务WIP，也未静默吸收Gallery后续私有整改。

## 本轮实际复跑

| 层 | 结果 | 范围 |
|---|---|---|
| pin身份与Cargo解析 | 188文件匹配；8个包来源均为精确Git rev | 不把仅写入SHA视为实际消费 |
| AgentKit UI | 30项通过 | 原037的28项 + 两项故障回归；含第二AgentKit公开组件consumer |
| 独立公开consumer | 131项all-targets通过 | 直接在固定Git checkout的examples/composition-consumer运行；10个本地包全部在该checkout内 |
| 默认headless | 76项通过 | 与UI构建隔离 |
| 共享目标回归 | controls/platform/text共287项通过 | 直接测试公开pin中的库，包含wake并发/关闭、文本/阅读/输入契约；不是重跑全Gallery |
| 严格Clippy/格式/构建 | 三套all-targets Clippy、root/UI fmt、demo build通过 | 根/UI所有权检查通过 |
| 工具回归 | 16项通过 | 新增正式pin不允许混用path、移动rev、重新capture的保护 |
| 两项故障回归 | 通过 | 分页首次超时后显式同cut/frontier重试；通知目标关闭后原Run继续结算且echo恰一次 |
| GPU离屏 | 构建出的当前demo渲染成功，图已查看 | PNG SHA `819c3c0d9415b7c17eacec9b47825dfd19c897e817c241e6b4a396d48ac05fc5`，与037相同 |
| 真实Native SEP → GPU | 全生命周期、提交/取消/去重/重连/历史、当前demo输出通过 | Kernel0.9.1，临时HOME及合成provider；无付费模型、无原生窗口操作 |

测试/metadata/Clippy/构建日志前缀 `/tmp/agentkit-formal-pin-`；机器收据含路径和SHA。统一复用 `/tmp/agentkit-final-receive-target`，未复制target。没有新开原生窗口、重做物理IME、VoiceOver或性能；接收的原生/物理/人声证据均保留原源/二进制身份。公开188输入未变，因此不为新GitSHA伪造现场重跑。

## 逐项关闭结论

62条当前GitHub正文与原验收矩阵逐条核对一致。先前55条技术/分层证据已独立接收；本轮固定公开pin并完成实际消费，补齐7条公开pin交付条件。详见 [逐行JSON收据](tekesui-public-pin-verification-20261019.json)。

| Issue | 已满足的重点 | 原全部条件 | 关闭建议 |
|---|---|---|---|
| #15 WindowWake | 固定来源、通知与流式/取消/连接回归、wake库并发/关闭、已接收原生双窗/idle源绑定证据 | 满足 | 可关闭 |
| #16 前插锚点 | 实际Earlier连续两页/宽度字号重排/选区焦点/A-B-A与删除过期映射；原生分页证据和公开pin | 满足 | 可关闭 |
| #17 输入/IME | disposition/修饰键/repeat/Tab/多行/提交隔离；接收原物理键和最终037物理组字记录 | 满足 | 可关闭 |
| #18 受控事务 | 同值ACK、composition/revision冲突、undo、晚到更新/草稿隔离；物理无REMOTE覆盖记录 | 满足 | 可关闭 |
| #19 指针选择 | 元数据、字素/多击/拖选/capture/次按钮/失焦取消；工具原生链与硬件限制已分开记录 | 满足 | 可关闭 |
| #20 文本视口 | Fill/Grow/clip/caret/候选与AX几何、readonly命名剪贴板、独立进入/逐行VoiceOver的人声记录 | 满足 | 可关闭 |
| #21 类型化动作 | ID/source/顺序/一次drain/禁用/重挂；原目的地Intent与真实消费回归 | 满足 | 可关闭 |

限定：#19的Down→失焦→Up是工具驱动原生链，OS pressedMouseButtons=0，不改称硬件鼠标按住；#20的named AppKit pasteboard不改称跨应用通用剪贴板。性能、全主题与其他组件族未纳入原条件，不因七项关闭升级成熟度。

## 公开CI的独立边界

[公开CI 39217768350](https://github.com/TekesApps/TekesUI/actions/runs/39217768350) 对应同一4310937：设计门禁、fmt、严格workspace Clippy通过；workspace测试失败于未随本次修改的Gallery用例：

`gallery::snapshot::tests::original_three_page_content_matches_the_committed_proposal_review`

日志：`primitives-light@2x.png` 的pixel `(652,145)`，`[252,252,251,255] -> [242,242,241,255]`。Gallery子测试结果为287通过/1失败/1既有忽略；后续CI中的公开consumer和精确身份步骤因此未执行，不能称其远端通过。本会话另外独立执行了公开consumer131、身份188校验及相关共享库287回归并通过。

该golden失败的根因由上游继续核查；未证明其无害，未改golden、关闭检查或把本地全绿冒称远端全绿。#15–#21原条件要求的公开API、功能/分层证据、固定公开输入及实际消费已满足；其范围不要求Gallery全库golden或其他整改Issue全部完成。故本次逐项可关闭结论与上游整体CI未绿并列保留；不据此建议合并/发布完整TekesUI稳定版。
