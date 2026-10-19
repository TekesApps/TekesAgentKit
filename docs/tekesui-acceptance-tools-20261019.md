# 等待期消费工具验证（2026-10-19）

本轮仅实现 AgentKit 的验收/pin 比对工具、修复归档边界检查入口并增加两项异常回归。没有修改 TekesUI、Kernel、production presentation 或既有 RunSurface，实现不依赖新 UI 能力。没有替换原 e4 冻结依赖，也没有改写037归档，未提交、推送、tag、关闭 Issue或发布。

使用方法见 [工具说明](tekesui-acceptance-tools.md)。

## 已完成

- `verify-tekesui-handoff.py`：新目录校验/解压、188公共输入、545 payload、三套包内 metadata、边界检查、fmt、三套测试/严格 Clippy、demo build、可选离屏 GPU与最终源码复验。逐步收据和失败/超时日志保留，target共用。不启动原生窗口、不提升候选。
- `check-tekesui-pin.py`：不带凭证查询公开 GitHub commit，临时 bare fetch固定40位SHA，按已审阅 manifest逐blob比对；改变、缺失、symlink、新增消费crate输入均阻止直接沿用验收。不读取活跃WIP，不改依赖。收据明确不授权发布。
- `check-headless-boundaries.py`：支持无 `.git` 的源码包和显式 root/metadata，不借用父Git清单；原依赖/无Swift规则保持。UI边界脚本也支持显式root。
- 两项真实 SessionRuntime/RunController故障回归：分页超时后内容/草稿保持、同cut/frontier显式重试恢复；assistant流式中关闭通知目标后Run继续结算，echo恰一次。
- 14项工具单测已加入CI定义；只在临时目录生成Git测试夹具，不联网。CI修改尚未推送，不声称远端已运行新检查。

## 本轮实际验证

| 对象 | 结果 | 证据 |
|---|---|---|
| 工具失败/身份边界 | 14项通过 | `/tmp/agentkit-acceptance-tools-tests.log` |
| 一键复现037封存包，最终r2 | passed，全部步骤成功 | `/tmp/agentkit-tools-replay-20261019-r2/receipt.json` |
| 037包内测试 | UI28、公开consumer131、headless76 | 上述收据各tests步骤及日志 |
| 037构建与严格Clippy | 三套Clippy和demo build通过 | 同上 |
| 归档headless边界 | `inventory=archive-walk`通过，不再需要临时Git或等价替代检查 | 同上headless-boundary.log |
| 037GPU | SHA `819c3c0d9415b7c17eacec9b47825dfd19c897e817c241e6b4a396d48ac05fc5` | 同上offscreen.png/receipt |
| 当前primary e4 | UI29（原27+本轮2），严格all-targets Clippy、root/UI边界通过 | `/tmp/agentkit-tools-primary-ui-{tests,clippy}.log` |
| 公开pin差异拒绝 | GitHub匿名公开查询成功；固定公开commit `80d6b05649234f953ea452d42e1cbed369fdddbb` 与当前e4 manifest比对为review-required：35改变、30缺失 | `/tmp/agentkit-pin-rejected-current-public-20261019.json` |

公开commit的拒绝验证针对**primary e4**，不是把它当作037的正式pin测试；037仍只有封存开发包。本轮分开记录了原封存037包与primary新增测试，未混报源码计数。首次r1成功记录保留；随后修正进程启动失败收据和宿主target环境隔离，用最终runner重跑r2通过。

工具收据含工具文件SHA、精确输入、命令和退出码；复现仅用一个 `/tmp/agentkit-final-receive-target`，未复制target。源码归档与188输入前后不变。最终pin工具另补中断失败收据，原Git字节比对及网络行为不变；14项工具回归再次通过。

## 保护与后续

本轮开始时130项原文件的保护清单保存于本机工具基线目录；仅修改预期工具/CI/gitignore及README文档入口，其他既有WIP逐字节保留。原冻结身份仍为 `e4c6cb6f725ff9df5ec4688927dd265d64dbbdb2380572d963c5b1017dde5bbc`。

这些工具为后续正式pin提供可重复比对和重验，不解除公开pin阻塞、不关闭 #15–#21。原生、物理IME、VoiceOver和性能不在本轮执行范围；沿用此前报告的分层边界。三端新UI宿主和任务等UI未扩展。
