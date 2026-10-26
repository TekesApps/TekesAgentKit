# 工具连续分组的来源输入边界

本阶段对照 ClientKit `TranscriptMetalActivity.swift` 的逐 call 时间线、`continuousToolEntries`
和输入间分段，补齐原嵌入工具控件在组成连续组时遗漏的用户输入边界。
Tekes host 仍只组合共享组件；本轮只修改 AgentKit，不修改 TekesUI、ClientKit、Tekes 或 Kernel。

## 共享契约与组件行为

`TranscriptSection::has_input_between_tools(first, second)` 读取原 `event_order`，
返回两个已定位 call 之间是否存在已定位 User 记录。使用 journal sequence 或 native ancestry/block position，
不使用时钟、正文、UI 下标、迟到 assistant 的整体记录位置来猜调用顺序。
任一 call 缺少公开位置时返回 None；相同身份或来源顺序冲突会报错。
这只回答输入边界，不证明完整语义连续性，不授予 session pointer、读取或提交权限。

原 OutputMessage 的 preflight 和实际 ToolRun 构建共用该规则：
原先相邻的正文工具块若跨越真实用户输入，不再组成同一连续组；
tool result 不作为用户输入边界，仍可保留其后的连续工具组。
无已证明边界时保留原 legacy 处理，不能据此声称未知来源已经完成定位。
各 OutputPart、disclosure、详情来源和原始 ID 保留；没有重写 DTO 或生成替代工具控件。

该 API 是未发布的开发增量，不能只固定 v0.3.8 就调用；宿主应固定包含本阶段的完整 AgentKit revision。
TekesUI 仍精确固定公开 `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`，本轮核对公开 main 未变。

## 验证

共享固定 JSON 模拟标准 journal：a 调用 → 用户追加输入 → b 调用 → a 结果 → c 调用，
最后同一个 assistant 消息提交 a/b/c 三个工具块，时间戳故意逆序。
Root 验证 a/b 有输入边界、b/c 无输入边界、反向查询及缺失身份；另验 native 同记录工具和未知来源。
第二公开 API consumer 与原 OutputMessage 验证 a 保持原单独 part，b/c 组成同一原 ToolRun，
原 disclosure 对象、part ID 及再次发布保留。
完整根仓库 310 通过，UI 607 通过/1 原有忽略；两套严格 all-target Clippy、格式及 Rust 边界检查通过。
同一冻结 UI/headless 程序通过真实 Pi 1.3.0、DSH 0.2.1-rc.2、Native Kernel 0.9.3 UI-only 的
协议、历史分页及离屏 GPU 验证；原收据 complete/rich/native false 保留。这些真实协议场景不产生
本固定 fixture 的多 call/追加输入组合，不能替代专门组件用例，更不能算原生点击或分页锚点通过。
日志、固定 fixture、源码差异和精确程序身份在 `.runtime-fixtures/tool-input-boundary-20261023/`。

## 仍待完成的物理分区

**消息级 Tools fragment 仍未拆成可独立参与父时间线的原工具行。**
本阶段阻止了跨输入合组，但不能据此声称 a 与 b/c 已分别落在输入前后，也不能证明 Previous 的完整分区计数。
后续必须移动原工具节点/分组，扩展 fragment 的所有权、prepared-plan 身份预测与重组检查，
让每个实际连续组使用自己的来源顺序，再验折叠、阅读锚点、复制与原控件保留。
不能改为克隆 ToolActivity、篡改源记录位置或由宿主补偏移。

7bfa0ba 的两项核心 CI 成功、两项 UI CI 均在 #53 严格 nested-detail 步骤失败（两个官方 job API 已核对）。
当前程序的原生窗口与中文组字尚未验收；上一次 activation 失败不重试或算成功。
#25 富内容、#48 正式同/跨进程 ProcessContainer、#53 嵌套阅读及完整 ClientKit 布局仍待完成。
PR #10 保持 draft，不合并或打完成 tag。本轮复用共享 target，未删除证据、固定快照或验收程序。

当前拆分机制仍是 Reasoning/Tools 两类 fragment 和 owner-local 固定 offset，
原消息的 split mask 与 parent preflight 要求两类原控件完整归还。下一步拆工具行必须一起调整
原节点归还、工具组身份、命名空间预测和借用校验；不能只搬节点而绕过原消息完整性检查。

后续已推进原工具组物理行、完整节点归还和迟到引用去重，见[物理工具分区](rust-agent-ui-physical-tool-fragments-20261023.md)。本页保留原阶段程序与验收范围。
