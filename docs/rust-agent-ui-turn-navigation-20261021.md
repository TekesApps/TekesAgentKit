# RunView 的原回合导航契约与 #28 候选消费

本轮继续对照 ClientKit 的 `RunView.jumpToTurnFromIndicator`、
`ThreadTranscriptPresenter.scroll(toSection:)` 和 `TranscriptIndicatorRailView`。
Tekes 宿主只提供导航/窗口能力；不修改 ClientKit、Tekes、Kernel 或 TekesUI 源码。

## 正式业务契约

`RunView::prepare_turn_navigation(&TranscriptTurnTarget)` 从原来已安装且已安排的
Transcript 捕获 `RunTurnNavigation`。宿主必须在当前帧成功 render 后调用；
geometry/publication 存在本身不是 native 帧可发布的证明。

`RunTurnNavigationKind` 表达产品语义：

- `TurnStart(item)`：普通回合，目标是该回合第一条真实保留行的**起端对齐**。
- `Latest`：rail 的语义最后回合，走已有 return-to-latest，包含 input 为空的
  infrastructure 尾部记录。不能只把最后一个输入气泡揭示为可见便算完成。

原 `TranscriptTurnTarget` 保留 scope、publication、epoch、turn ID 和实际 Content IDs；
不会读取 endpoint 或制造新回合。手势的 clone 共享一次 dispatch admission。
`RunView::claim_turn_navigation` 在实际库调用前检查原组件 owner、publication/epoch、
当前行成员、Run authority generation 与 pane 生命周期。折叠→重开、同 publication
的 authority ABA、更新和重连不会让旧手势获得新来源。

这是不改变焦点的本地导航，允许保留 Composer 的 preedit。旧
`validate_turn_target` 的一般控制/组字 gate 没有被放宽；导航专用方法不因此授予
发送、修改、切换会话或系统能力。库仍负责滚动映射、命中、动画、IME 和原编辑器状态。

第二公开 consumer 提供了相同的 prepare/claim 接入函数，隔离 RunView 探针实际调用
这两个公开函数，不引用 demo 私有类型或重新推导 turn/row 身份。

## 顶端对齐缺口：不能把 centered 当成正式实现

当前固定公开 TekesUI 是 `8cf0840790334ba224afc0595897c74231c712e6`，仍无稳定 item
导航公开入口。新消费的 #28 隔离候选是
`10b5f2354f76a1c250143ef15ea7f7337f7be632e0b979ff00feb569338b5481`：
486 个文件、基于未改动 292 的18路径增量，归档 SHA256 为
`361318c1ee6995dc08b3ca7a489340616e43dbc823a032821e43756d168b04f9`。
文件、差异和 tokens/sets/api 值在接收侧重验；不跟随 producer WIP。

该候选提供 nearest/centered，尚无 item 起端对齐。ClientKit 普通回合明确定位到
turn top（并非 minimal-visible 或 centered），因此新增了
[TekesUI #32](https://github.com/TekesApps/TekesUI/issues/32)。正式普通回合入口保持
未接通，不通过反算 offset、伪造 margin、借 focus 或消费方文字/滚动引擎绕过。

隔离探针中的 centered 适配器**只验证基础 API 在真实 RunView 上是否保留来源和
编辑状态**，不作为 TurnStart 的产品实现，也不证明 ClientKit 导航布局已对齐。
原生 diagnostic 同样明确标记 `clientkitTurnStartAlignment=false`。

## 阅读与显示责任

宿主保留原 TekesUI `ReadingDocumentVersion`，SDK 调用使用原 viewport、版本、稳定行 ID
和原 renderer。canonical 更新仍以原 `RunViewUpdate` 和 library reading transaction
为一个同步事务；append/历史 prepend 不作为第二次显式导航。SDK 版本 ABA 的拒绝与
Agent 手势身份的拒绝分别检查。

每次 accepted 请求，即使 changed=false，也需要重新 render，成功后再发布当前
native AX/input/geometry。失败不能通过另一个 editor 或另一个 pane 重试。
被拒绝的手势需要新的显式捕获；不自动重放。

SDK 的 item mount 绑定需要 owner 已建立有效 reading 布局。接收侧嵌套检查先让
owner 部分可见并布局，再揭示其远端目标；初次 owner 完全被祖先裁剪时实际返回
UnlaidOut，不把该失败尝试计作已支持的首帧隐藏 owner 导航。已布局后的显式 clip
另验拒绝，原 offset/编辑状态保留；没有在消费侧越过该前置条件或修改库。

初次 latest、rail follow-target 变化、同目标去重和用户手动浏览的业务规则仍归
AgentKit。该份收据记录时真实独立 rail 尚未接入；后续[实际轨道组件](rust-agent-ui-turn-rail-controls-20261021.md)已增加原按钮命中、事件捕获、独立滚动及 follow 去重。hover/预览、#32/#33 与完整体验仍待完成，不能把普通程序导航 diagnostic 当作完整 rail。

## 验证与限制

同名 JSON 记录实际终态、source/lock 身份、组件/第二 consumer、原生窗口和失败尝试
的分层结果。默认公开路径整包346项，最后符号命名明确为 TurnStart 后又通过相关11项、
独立 consumer34项与 all-target Clippy/build；候选整包359项，包含原8项事务回归与
新增5项导航检查。SDK controls388项、text执行49项（1项按名称过滤，hard-lines GPU
测例随包执行）、consumer206项；专项25项属于其子集，不累加。不宣称重跑全部 GPU
矩阵。SDK 三包 strict Clippy、native example build 和解压内依赖闭包检查通过。所有 fixture 都在本地独立来源内，不调用模型，不读取 harness 私有数据库。

原生 diagnostic 使用原 RunView、原 TextBackend、原 Composer，初次返回 latest，
经 AppKit bridge 输入合成 preedit，再程序调用 centered reveal 和显式 latest，随后
成功绘制/发布。它不表示物理中文 IME、真实 rail 点击或真实三端 UI 验收。

未完成目标保持：#32 起端对齐、#29 真实 rail、完整 rich Transcript、阅读/分页三端
原生闭环、物理 IME/VoiceOver、配对产品视觉和全部 Views 对齐，以及最终 review、
CI、合并与阶段 tag。默认公开 pin 和既有冻结没有提升；本轮不建议据此提升冻结。
