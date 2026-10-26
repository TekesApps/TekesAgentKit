# TekesWebUI 接入兼容记录（2026-10-26）

归属：Epic #14 / #37（AgentKit Remote host contract）。这是当前源码审计记录，不代表服务已发布、WebUI 已安装新二进制，或已完成浏览器验收。

## 固定参照与实际消费边界

TekesWebUI 当前 `package.json` 与 `package-lock.json` 都把 `@tekesagentkit/client-contract` 固定在 AgentKit commit `a8610a420bad72eaedcf706b30df87153a0f3e16`，安装包自报版本 `0.3.7`。这是 WebUI 的公开 DTO 声明来源，并非它运行时 `tekes-agent-web` 二进制的版本约束。WebUI Host 配置单独引用 AgentKit binary，运行时真实版本应从该进程的 `endpoints[].host.product.version`、Host ready/provenance 和实际文件 hash 核验。

WebUI 已有 Node host → 私有 stdin/stdout JSONL → `tekes-agent-web` → `SessionRuntime`/Harness 的桥接。HTTP 服务负责本机鉴权和 NDJSON 转发；AgentKit 负责 endpoint、session、presentation 和 Harness 语义。浏览器端通过 AgentKit 的公开操作使用服务，没有另建会话或协议状态机。

## 当前接口对照

- WebUI 的 HTTP 操作集合 `endpoints/describe/snapshot/call/follow/history/respond/earlier` 均由 AgentKit 服务实现。Host 私有 `configureDSH` 与观察取消 `cancelObservation` 分别保留在 Host/JSONL 层，不作为普通 Harness 方法暴露。
- WebUI JSONL bridge 发送 transport `id` 和 `op`；调用方另带稳定 `rpcId`。AgentKit 以 `{id,value}`、`{id,error}`、`{id,done:true}` 回复。follow 帧仍是 `{id,value}`，HTTP host 去掉内部 id 后转 NDJSON；关闭观察只取消对应 follow，不停止 Harness 任务。两层 correlation id 不应合并。
- 当前 WebUI 从固定包导入的 16 个 DTO 名称都存在于 0.3.7 声明中，包括 annotation、thread actions、pin/activity、live/reported usage 与权限类型。调用者仍消费 AgentKit 的 HostDescription、SessionView 和 endpoint method capabilities；这部分 JSON 协议不由 TypeScript DTO 包完整描述。
- 候选 AgentKit `2c8a2fb` 相对 main `1dcc521` 的 Web service 新增 sidebar 读取实现，并将现有 JSONL envelope/request helper 提取到 `stdio.rs`；操作和 wire envelope 未见破坏性变化。`ComposerAnnotation` 新增可选 `artifactReference` 字段，旧 WebUI 继续发送原字段即可。该新属性目前不在固定 0.3.7 类型包，也不被 WebUI 导入或发送；若未来 WebUI 使用 artifact selection，需要发布新不可变 contract pin 并同步浏览器类型。

## 复核命令

从 AgentKit 仓库运行：

```sh
python3 scripts/check-web-consumer-contract.py \
  --agentkit /Users/frankqin/dev/TekesAgentKit \
  --webui /Users/frankqin/dev/TekesWebUI
```

检查会比对 WebUI manifest/lock 的固定 commit、已安装 DTO 声明中的实际类型导入、HTTP 操作、AgentKit JSONL envelopes、取消观察和 mutation rpcId 传递，并报告 main 到当前 checkout 的 Web service 文件变化。它是源码契约检查，不代替编译、独立 consumer、真实三端协议或浏览器操作验收。

## 当前结果与未验证项

在本次只读源码比较中，未发现阻止现有 WebUI 消费候选服务的 JSONL/API 兼容差异。检查器运行结果为：固定 pin `a8610a4` / package `0.3.7` 一致；16 个已使用声明存在；8 项 HTTP 操作、JSONL envelope、cancelObservation 与 mutation rpcId 对齐。

仍待完成：将本阶段 AgentKit Web/headless 精确版本构建、集成并发布；确认 WebUI Host 当前保存配置实际指向该程序；再以发布后的精确 WebUI 与 Harness 运行浏览器端到端验收。以上检查未验证当前本机运行服务、真实模型、附件、审批/提问、浏览器关闭/重开或 service restart 行为。Kernel #86 仍是 Native busy queue 的受支持能力失败；其职责在 Kernel，不改变 WebUI 到 AgentKit 的 JSONL 契约，也不能据此宣称三端队列已通过。

## 审计纠正：实际安装内容并非公开 pin

进一步逐字节核对发现，node_modules 声明 SHA256
`7aca3df0e5816bf88d1385d013ab506400a9b4a74844441db0bde772e87eed21`
与 a8610a4 公开归档声明 SHA256
`f92ff73d7e5d7181e3bdc29f2e7155d000cdf9381358e2614b8a87f414381e62`
不同。包自报版本相同不能证明实际依赖匹配。上文“16个声明存在”仅适用本地开发覆盖，
不能认定其公开 pin 可复现；PermissionPolicy/Option/State/ModeState 尚在另一 worktree 的 WIP。
本次整合复用该既有实现并发布新不可变契约后，再更新 WebUI pin、移除覆盖并重新检查。
检查脚本现按 git blob 严格比较实际安装声明，不再凭自报版本推导成功。
初始 WebUI 的91 pass/1 skip与构建是开发覆盖基线，不是最终公开consumer验收。
