# 不依赖当前 TekesUI 缺口的 ClientKit 核对

本轮基于 `6062079` 检查现有状态、来源和异步回调，修复 AgentKit #12/#13。
只消费固定公开 TekesUI，ClientKit/Tekes 为只读参照；没有 Swift、Kernel 或 TekesUI 改动。
设计与 API 迁移见[来源契约](rust-agent-ui-source-authority.md)，关联 PR #10。

## 检查与实现

| 范围 | 对照与检查结果 |
| --- | --- |
| Run/Composer/连接与异步准备 | 原 RunController、捕获目的地和准备票据继续唯一拥有状态；来源在准备前后检查，不自动重试。派生来源修复通过既有 gate 覆盖模型/反馈/附件/操作回调，不复制状态机。 |
| Task/子代理/SideChat | 对照 ClientEmbeddedRunView 的移除即 stop、ClientAgentTaskView 的动态 pointer，以及 SideChat 的 Closing/Unknown/absence。发现并修复 #12：for_run 派生绑定继承原 Task/child/SideChat 权限，不同 scope 也不能跳过；异步 history 返回后再次校验，旧源不复活。主 Run 草稿和同目标 presentation 共享保持。 |
| Transcript/只读历史/分页 | 发现并修复 #13：scope/epoch/publication 相同不能证明同实例。snapshot→组件输入→intent 保留 opaque 来源身份；错误投递在 claim/IO 前拒绝，原 gesture 仍可在正确源执行，clone 仍只执行一次。宿主缓存写入前核对实例；替换来源须挂载新组件。 |
| Sidebar | 保留现有稳定 action value、捕获 source/token、目录/查询游标、选中补齐及偏好 CAS；现有跨连接、旧来源、过滤分页及动作回归通过。本轮未改操作状态机。 |
| Summary/任务动作/反馈/附件与预览 | 检查原 scope/target、publication/epoch、CAS、迟到回包和取消清理；模型/预览及写入入口仍使用原绑定。没有从当前选择、标题或私有文件猜 pointer/report，没有发现需在这些入口另建状态的差异。既有回归保持。 |
| 组件组合与阅读 | RunPort/Transcript/只读 pane 继续用原 preflight 和 SDK 阅读事务。新增实例身份只管业务来源，既不改变 scroll/IME/焦点引擎，也不冒充持久 checkpoint。 |

#12 用红测试复现了父 child 退役后嵌套 child 仍 Ready、跨 scope Task 仍 Ready，以及
Task parent 在异步 baseline 中取消后 child 仍可返回。修复后还检查准备票据拒绝/无远端提交、
父来源 A→B→A、SideChat Closing/Unknown/absence、原主 Run 非空草稿和第二 consumer 的可见性。
弱 owner 与原 controller 引用被去重继承，不保留旧 view；缺失来源不能退回父正文或空白新 Run。

#13 的红测试复现了同 scope/epoch/publication 的两个实例能串用 snapshot 和 read intent。
修复后增加 None/新建伪来源拒绝、错投不消费原 admission、正确源刷新及 clone 只执行一次。
没有全局 counter/registry，没有把来源身份改写成 publication；原分页 epoch、重连显式刷新、
只读门禁及旧帧/无效数据不能污染缓存的规则均保留。

## 验证与程序身份

- 完整 UI workspace **628 通过 / 1 项既有忽略**，包括共享组件/绑定、demo 宿主和第二公开 consumer。
- 补充 display-only/假身份负例后，全部只读 **14 项**再次通过；业务实现未在全量通过后继续改动。
- 严格 workspace/all-target Clippy、格式、UI 边界检查通过；Python **74 项**通过。根 crate 未修改，本轮不重复根测试。
- 冻结 demo SHA-256：`8f13b993a77bd2556871e10d17b88b33501d3b30354e5e2354bf38d0cba882ae`，两项真实专项共用该程序。
- Pi 1.4.0、DSH 0.2.1-rc.2、Native Kernel 0.9.3：常规公开协议/数据回传和离屏组件路径通过；Native 本轮实际完成完整协议驱动场景与有界子历史专项。
- Native 有界 pane：真实 task 来源、源调用只读详情、单消息分页、原读取 intent、控件保留、重连、非空父草稿隔离、无 Composer/拒绝发送通过。
- 原 SideChat 专项：Native/Pi 创建、原编辑器合成 preedit/commit、发送、切换草稿、同实例重开、重连不重复提交、确认丢弃通过；DSH 无 discard 能力，只确认禁用门禁，不能声称其 SideChat 可用。

红/绿测试、原始协议与 consumer 收据、自绘 PNG、程序、源码和锁快照保存在
`.runtime-fixtures/pane-parent-authority-20261023/` 和 `.runtime-fixtures/readonly-source-identity-20261023/`。
精确源码/程序/文件摘要见[机器记录](rust-agent-ui-non-sdk-review-20261023.json)。
构建复用共享 target，程序用 APFS 写时复制归档且 inode 独立；没有新增 target、删除历史证据或签名安装宿主，实际释放 0 字节。

## 本轮未验收与剩余依赖

本轮没有桌面窗口、屏幕截图、物理 IME、VoiceOver 或 ProcessContainer 同/跨进程验收。
离屏合成 preedit 不等于系统中文输入验收。故障和迟到回包的确定性负例不等于真实网络故障注入。
`completeRunPort`、`completeTranscript`、`richContent` 仍为 false；不以本次状态修复声称完整 Views 对齐。
#47 图标/紧凑动作、#48 交互式 ProcessContainer、#53 严格嵌套阅读继续保留；其他富内容、
持久 checkpoint、非当前行上下文菜单及最终配对布局仍按交付状态列明。
没有通过改变布局、伪造来源或放宽 CI 来绕过 SDK 缺口，不合并、不打完成 tag。

## #13 补充：纯展示输入的读取控件

来源权限复核发现 `source: None` 的纯展示输入仍可能启用刷新/分页，再由绑定拒绝无来源请求。
已在原组件门禁要求非空来源，不新增状态；纯展示输入禁用两项控件且不产生 intent。
回归明确设定 ready、current、支持分页及有更早记录，避免误把无页禁用当作来源门禁；
`Some` 假身份仍产生请求并由原绑定拒绝，真实来源读闭环保持。
主会话协调、subagent 实施，完整 readonly_child 14 项、格式和严格 all-target Clippy 通过。
日志为原证据目录内 `source-authority-none-*`；这是增量组件/公开 consumer 验证，
没有重跑三端离屏或完整 UI 628 项，也不将先前冻结程序的身份转移到本次源码。
两套 9def8e6 CI 已完成：headless/UI 测试/格式/Clippy 通过，均在 #53 严格阅读检查失败；
原 JSON 收据分别保存在同目录 `ci-push-39930931314/`、`ci-pr-39930940983/`。
桌面、物理 IME、VoiceOver 与 ProcessContainer 仍未验收。
