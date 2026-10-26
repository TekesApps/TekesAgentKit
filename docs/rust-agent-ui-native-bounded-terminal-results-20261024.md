# #17 / #23：Native 有界 Task-source 终态修复

按[契约](rust-agent-ui-native-bounded-terminal-contract-20261024.md)，基线 `20b4c44` 的 built-in bounded Transcript 合成 turn 现保留公开来源的错误/取消：`sourceCall.phase=error` → `status=error`，`stopped` → `aborted`。running/done 和未知phase的旧fallback不变。原 tool.phase、call identity、arguments、child entries、run_open 和父子指针均未改，没有新状态机或协议。

来源依据为只读 Kernel **v0.9.3 tag `09508a9083c24c7accefc99516739f279b85913e`** 的原 bounded reader。它明确将 child settle 的 completed/interrupted/其他结算/未结算映射成 done/stopped/error/running。这不是对当前 Kernel worktree WIP 的推测，也没有更改或重建 Kernel。

新子代理只补公开 SEP fixture 回归；主会话先运行，2项均红，原日志明确 error 被错误标为 completed，再实施映射修复。最终通过公开 `ChildRun::open(SameSession)`、同源 refresh/Transcript 观察4种已定义phase及未知phase兼容；同一个 ChildRun 的 running→error / running→stopped 保留 opaque ID、tool身份/参数/原phase和独立历史。历史字段固定用于检验phase不被其他字段替代，不能当真实Kernel转换的证据。

| 验证 | 结果 |
| --- | --- |
| 定向红/绿 | 原2失败，修复后2通过 |
| Root 默认 | 373/0/0 |
| Root presentation feature | 281/0/0 |
| UI 默认 | 757/0/1 |
| Clippy / 格式 | Root默认/feature、UI严格检查通过 |
| 普通/feature demo | 构建通过，未启动 |
| 独立公开组件 consumer | 实际CPU执行通过，含RunPort/Sidebar/SidePanel等原公开组件 |
| 边界 / 验收矩阵 | 通过，旧失败不豁免 |

独立consumer是显式当前源码开发路径；准备时receipt正确记录WIP，137文件闭包 `ec0787e887c7af9641f8ae7811ddef90dd570810c55a38278a2addee5dfa2a5a`，提交后另以immutable tree核对。SDK精确pin不变。没有修改Python，复用其255/0证据。

`.runtime-fixtures/native-bounded-terminal-20261024/` 保留原Kernel参考文本、失败程序/日志、最终2文件哈希及程序、测试/构建/consumer日志、固定锁与输入，见[机器收据](rust-agent-ui-native-bounded-terminal-results-20261024.json)。Cargo串行复用SSD shared-target，无新target、删除0字节；源码、WIP、验收程序和现用缓存保留。

本轮没有真实Native同一原pane的running→failed来源捕获，也没有桌面/物理IME/OS AX/ProcessContainer或完整ClientKit验收。前轮Pi/DSH actual child pane证明自己的路径，不能替代Native。下一切片在原有界读取及原publication上补只读来源见证，避免二次查询拼帧。#17/#23保持open，#53严格嵌套阅读仍未通过；新head CI另验，不合并/打tag/发布。

9039d5f CI终态补录：push40105849539、PR40105854722的checks、bindings/demo opt-in与独立consumer通过，唯一失败仍#53严格阅读，CI demo跳过。两份原日志/artifact、137/243闭包和PR merge父关系已核对；不能代替后续head来源见证CI。
