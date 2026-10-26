# #45 / #56：Native 被动工具 Summary 的来源边界

功能追踪 #25、Epic #14。组件来源适配归 [#45](https://github.com/TekesApps/TekesAgentKit/issues/45)，真实协议切片归 [#56](https://github.com/TekesApps/TekesAgentKit/issues/56)。不是为每个probe建票；本切片不关闭完整三端/正式Task要求。只消费固定公开TekesUI，不改Kernel/SDK/Swift，不跑桌面。

## 实际发现与原负向结果

`native-summary-transition-live-1` 是595输入冻结的真实Kernel0.9.3 + 合成loopback provider运行。原子pane的公开两页计划、一次Earlier到EOF、同source重连与错误prefix均通过；新增Summary子断言为负向，未将paging的accepted=true升级成Summary通过。四帧、原程序、完整595文件源码、公开事件和所有退出/失败收据保留。

原父工具在seq4 `tool/call`阶段为`doing`，被动条目原来显示pending；seq5 `tool/result`的`meta.kernel_outcome=ok`，内容为spawn句柄，父工具终态done/completed。子history的`sourceCall.phase=error`是另一份子运行来源，不推翻父工具成功。此前预期同一profile证明父Task运行→失败，是验收来源混淆。

## 最小修复与不变的完整要求

仅在SummaryFacts从明确`task.task_name`工具创建被动条目的来源边界，将`doing`适配成`in_progress`。Transcript保留原phase；正式AgentTask未知status规范不全局扩大。条目保持父scope、原opaque工具ID，不获得独立Run或写权限。

本profile改为明确验证**父spawn工具的in_progress→completed**，同时另证原子历史error；不能称为正式Task运行/失败生命周期。真正父工具失败必须来自其自己的公开result/error，正式Task实体/独立pointer仍须合法producer；两项继续列为未验收，不能用子失败或新的成功profile替代。

## 来源与验收

通过原父RunBinding的`refresh_with_source_witness`，在同一runtime inspect中取得snapshot和MemoryProjectionStore公开事件输入，要求projection成功、scope/epoch/ready和freshness一致。按原Transcript工具的sourcePosition/resultPosition匹配raw call/result及callId，不拆opaque ID。adapter后的Task status与原tool phase分别记录；成功/失败只能从对应公开结果判定，不从输出文字或child phase猜造。

切换原SidePanel Summary tab，保持原SummaryView/被动TextField控件实例和row ID；已安装帧必须等于同一父publication。保留父非空草稿、Run目的地选择token与binding；这不是文本caret/Undo/物理IME验收。输入捕获与安装若跨了新publication就显式重试，不拼接两帧。

独立Python verifier从raw authority事件核对原字段，拒绝陈旧/未就绪、epoch/publication错配、成功结果伪装失败、源错配、缺少实例身份。正式profile范围须有机器字段；父工具失败/正式Task生命周期接受标记保持false。原child分页/source receipt/公开计划/prefix/所有owned进程退出门禁不弱化。

先做受控红绿、共享库/宿主/第二public consumer验证，再重新运行同一真实producer的声明切片。只记录它实际证明的范围；glyph/桌面/OS AX/性能与完整View仍未验收。
