# #22/#29：真实三端同宿主 Sidebar 交付（2026-10-24）

## 已证明的范围

同一个当前离屏 Rust demo 连接隔离的 Native0.9.3、Pi1.3.0、DSH0.2.1-rc.2。每端经公开接口创建、选择模型、重命名并完成三个实际会话，逐 session 核对唯一输入和真实 harness 返回的受控回复。共九个实际 ID，不写私有历史或用户目录。原 SharedAgent→SidebarHostBinding→ComponentDemo→Sidebar 链路完成：

- 九行来源地址/title/cwd 与实际 seed 一致；每个 endpoint 都 loaded、converged、非 stale、无错误，不能用 aggregate loaded 遮盖失败端。
- 同一组件原分页控制的 semantic Press 经原请求/load_more 路径，把 limit 5→10、挂载行 6→9。已有行 ID 不变，没有 endpoint mutation 命令。
- 同一宿主完成三端公开搜索；实际 accepted display projection 的本地及 provider readiness 均为 true，unresolved 为空。raw reader unresolved 与 display projection 分开记录。
- 原主会话导航建立组件 reveal target；逐阶段检查该 item 确实存在于 ContentHost，并匹配实际源行。分页/查询保留原 Composer 焦点。该证据只证明目标挂载，不证明桌面滚动或像素可见性。

新入口与责任边界见[契约](rust-agent-ui-real-multi-sidebar-contract-20261024.md)。`--sidebar-delivery-receipt` 是离屏验收入口，未放宽原单端 proof/selftest 门禁，未增加产品状态机。

## 发现并修复的问题

1. SSD 卷名包含空格时，旧 provider 在 JSON 序列化字符串上用正则提取 cd 路径，导致 DSH 工作目录探针截断于 `/Volumes/HP`。现在从实际 user content 解码完整路径；空格、中文、引号/反斜杠保留，忽略 assistant/tool 引用和非法最新标记。原失败和 DSH 修复重跑证据都保留。
2. 加强真实 profile 后发现：原分页按钮默认抢焦点，最后一页删除按钮后焦点停留在旧按钮 ID。先保存失败 receipt/红测试，再让 AgentKit 按钮使用固定公开 SDK 已有的 `Content::preserving_focus()`；SDK 继续拥有输入、渲染、Tab 和显式 Focus。未修改 TekesUI，也无需新增基础缺口 Issue。

Pi 配置、Native/DSH lifetime 和原 provider 已提取为可导入工具并接回旧脚本，原协议断言保留。新 profile 使用明确关闭继承的受控环境；Pi 仍由原 Rust connector 启动，显式清空它可能继承的本 profile Native/DSH auth 环境键。DSH 查询 SQLite 只在本隔离 profile opt-in，不改用户部署。DSH 持续排空 stdout、交换 cookie 不跟随重定向；Native 持续排空 stderr、bootstrap 后按原 stdin 关闭顺序停止。仅清理自己启动的进程；超时不静默算成功。

## 最终验证与身份

最终 UI workspace **717 passed / 0 failed / 1 existing #49 ignore**；Python **143 passed**；严格 UI Clippy、普通 demo build、实际独立公开 consumer 的五个场景均通过。最终三端完整无 UI 协议脚本再次通过，Native 包括 archive/unarchive、真实 Task 来源和只读有界子历史；这不是本聚合宿主已完成 Task-family 组件验收的证明。

主证据目录：`.runtime-fixtures/real-multi-sidebar-20261024/`（实际位于 SSD）。`aggregate-verified/` 是严格最终真实运行；`aggregate-attempt1/` 只验证早期较窄范围，不具备后来新增的焦点/reveal 证明；`aggregate-final/` 是保留的焦点失败。不得把三者结果互相转移。`source-verified/` 和 `verified-source-identity.json` 冻结本次实际执行来源；运行后复核未变。

当前 demo SHA-256 `8d053c0f198ff28cb8111cd881eff388da33cba9132e0e9e9fac005f3bd36a7c`；headless SHA-256 `87b54fe157a367ed462333e6781e839b5454cc88e1bb9543c8e1faf3a532e8ad`。固定运行入口/包元数据哈希已与 preflight 再核对，实际握手精确为上述三版本；这不冒充全部转移依赖文件的重新审计。demo 已另存为验收归档，不从归档程序启动桌面。完整程序/来源/命令/日志和 PNG 的身份见[机器收据](rust-agent-ui-real-multi-sidebar-results-20261024.json)。

全批复用 SSD `shared-target`，未新建 target。保留本批源码快照、实际会话数据、失败与通过日志、收据、离屏 PNG 和验收程序；本批额外删除缓存 **0 bytes**。结束后本批目录 `lsof +D` 返回 1、无 stdout/stderr；这只是没有发现打开句柄的观察，不是一般删除授权。原有无关 supervisor PID97755 仍在，未操作。

## 未完成与 Issue 状态

未运行桌面窗口、Screen Recording、物理中文 IME 或 OS AX。组件合成预编辑测试属于组件层；不能替代真实输入法。实际三端相同 session ID 冲突未证明，仍依赖既有混合 fixture；archived/ephemeral/child 全分类来源与保留域等价、远端 cursor/故障、Task-family 导航、ProcessContainer 和完整 UI parity 仍开放。

PR #10 保持实验状态。前一提交 `3ebf7de` 的 push/PR CI 都是 headless/工作区/独立 consumer 通过、仅严格嵌套阅读失败；本批新 head CI 另行核对。TekesUI #53 门禁继续保留，不合并、打 tag 或发布。#22/#29/#27 本批可回填上述细分证据，不能据此关闭全部标准；#12/#13 自身正文的 PR 合并前提也尚未满足。#23 旧 followup 已注明 `adbb7f3` 补齐 child-open 错误回归，真实 Task 家族与窗口标准没有被本批替代。
