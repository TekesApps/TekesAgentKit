# AgentKit Issue 体系整理结果（2026-10-25）

关联 [总 Epic #14](https://github.com/TekesApps/TekesAgentKit/issues/14)、[集成追踪 #58](https://github.com/TekesApps/TekesAgentKit/issues/58)、[PR #10](https://github.com/TekesApps/TekesAgentKit/pull/10)。当前入口是[需求覆盖索引](agentkit-requirements-coverage.md)与[机器表](agentkit-issue-coverage-20261025.json)。原正文、原状态及日期完整保存在[历史归档](agentkit-issue-history-20261025.json)，原专题/程序/失败收据不删除。

## 实际调整

- 保留 #14 为唯一总 Epic；#15–#28 为14个功能追踪，不再把实现、真实协议、桌面、SDK和发布混在一组待办里。
- 新增 #30–#59 共30个交付子票，复用 #12/#13/#29；形成33张实施票。每张都有范围、交付物、依赖、验收/关闭标准和负责人 @qf0421，并建立 GitHub 原生父子关系。整个树48张票、47条主父子边，关联功能通过引用复用，不重复实现。
- 按已发布的历史里程碑核准关闭 #30（三SEP/pointer/无UI Run）、#31（无UI Task/子代理/SideChat/Summary）、#32（Pi子代理/Native有界历史）。原PR均合入，并为已发布v0.3.8祖先；对应原CI终态成功和原完成UTC时间已经回填。v0.1.0首轮CI因账单限制未启动仍保留；没有重编译或把当前程序当历史验收。
- #29按新的集成门槛重新打开为“已验证待集成”：f399356仍不在main，旧三端来源证据有效但不是已合并。#12/#13也保持开放待集成。没有提前关闭PR #10代码项。
- 六种状态标签互斥：待办、进行中、已验证待集成、等待上游、等待桌面、已关闭；公开仓库与MIT当前事实也已核验。

## 发现和归属修正

| 问题 | 整理后的归属 |
| --- | --- |
| 大票混合实现与验收，已验证代码看似没做或被整票等待SDK | 基础组件子票 #33/#34/#38/#41–#45 及复用 #12/#13/#29 标已验证待集成；协议 #55/#56、桌面 #57、集成 #58、发布 #59 分开 |
| API、RunPort和验收重复列相同能力 | #33管公共契约/第二consumer，#40管标准Run组合，#55/#56管真实来源，不在每张票复制状态机 |
| Sidebar分类、父权限、只读身份已有对应票 | 复用 #29/#12/#13；#43只管面板内容/导航生命周期，#52只管非当前行菜单 |
| 最新ClientKit工作树的独立Composer与detach提交未被旧草稿测试覆盖 | #35 独立宿主/同文档拆挂/原submission协调，复用现有Rust ledger |
| 一般source_locator不能证明精确artifact选择恢复 | #36 精确引用、revision/digest/UTF-16和stale/unavailable解析 |
| Rust Web服务与ClientKitRemote宿主不等于已经完成跨宿主契约 | #37 WebUI/Remote公开协议与接入；Swift adapter仍归ClientKit，发现/Keychain归产品 |
| 模型/Agent设置能力与宿主配置回调容易遗漏 | 明确列入 #34/#35 的原控件/生命周期要求及覆盖表R43；不把产品scenario参数移进Kit |
| 富内容、glyph、popover、检查点、嵌套阅读、行菜单、IPC是不同上游交付 | #47–#53各等自己的公开SDK；位图缺口TekesUI #117明确归 #47 的图片子项。父功能与其余已可执行工作不被整体阻塞 |
| 报告/旁白尚无确认的公共producer，旧政策审计不能算真实数据源完成 | #54 等待实际公开producer；不逐harness制造空票，不猜prose/opaque meta |
| “性能未验收”只有声明、没有负责交付 | #55/#56负责非桌面延迟/大历史/内存；#57负责真实窗口响应/帧/jank/资源。先冻结profile/数据量/机器版本/量化预算，再测；未定标准不能关闭 |
| 新AgentKit #53与旧裸引用“#53”冲突 | 当前票明确使用完整TekesUI仓库链接；原原文留在历史JSON，不把旧SDK失败误指新ProcessContainer票 |

## 证据边界与下一步

AgentKit代码基线为1d41ad4；ClientKit提交c289bc9与当前未提交候选分别记录。当前UI790/0/1、定向default/feature各15及独立consumer通过，只说明各自验证范围。1d两轮CI只有严格TekesUI #53失败，整体仍failure；原artifact/log、138输入、SDK8包/243文件闭包和PR parent已经核验。固定1026格矩阵仍绑定91129d7，不升级为新HEAD。

当前33张实施票：已关闭3、已验证待集成11、进行中5、待办5、等待上游8、等待桌面1。原始要求全部保留为43条覆盖项；这不是完整UI交付或关闭Epic的结论。

整理核对后先继续 #56 已保留的Native Summary WIP（编译和原call/result验证尚未完成），再推进 #35/#36/#37 的新契约及 #39/#40/#46 的独立部分。所有Cargo由主会话串行，继续使用SSD shared-target。#57不跑桌面/IME/AX，不触发截图授权。阶段合入后才关闭对应代码票；#58不要求代码票先关闭，避免循环依赖。

本次只进行Issue治理、只读审计和文档修改。优先级切换前已启动的Cargo build以失败结束，日志保留；未启动新构建或新真实验收。五个Native Summary候选文件的SHA在整理前后核对，代码未混入本次文档提交；固定快照、原日志/收据/截图/可执行文件保留。未新建target，本次删除与释放空间均为0字节。
