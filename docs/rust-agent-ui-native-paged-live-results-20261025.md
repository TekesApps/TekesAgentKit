# #17 / #23：真实 Native 原控件分页分项结果

按[执行契约](rust-agent-ui-native-paged-live-contract-20261025.md)完成一次实际隔离 Kernel0.9.3 协议闭环。[机器收据](rust-agent-ui-native-paged-live-results-20261025.json)记录程序、固定输入、原始文件SHA、阶段与退出结果；PR#10保持试验状态。

原Sidebar/FamilyCatalog打开同一个page_size=1只读pane。原ReadOnlyTranscript Refresh/Earlier捕获原intent，沿同次累计witness publication回填实际组件；未手造intent或先读snapshot再补查来源。显式reconnect后原source/root保留，旧Earlier拒绝且没有额外Earlier RPC；新Refresh保留旧页的原readEpoch，随后原Earlier请求与独立公开pagePlan完全相同。父草稿、选择及binding保留，原Close后退休source，旧witness拒绝。

本次公开history因果组实际为两页：首page含seq1..21、四条assistant消息；Earlier只补seq0 turn/start，完整22条。重连epoch2→5；错误终态cut21→24、累计25条、挂载行12→15。不能据此宣称Earlier增加了可见消息或真实三次累计合并。sourceCall身份和工具/助手内容均与公开typed历史核对，最后错误可见。

实际provider通过四次公开read(proof.txt)正常执行并确认真实回传，第五次请求等待独立validator；全部pre-gate帧与独立公开full head/逐页请求响应一致后才返回HTTP400。原Refresh捕获错误新增事件及未合并PrefixValidation；中间head可能分批增长，每个prefix须绑定紧邻旧cut/revision。prefix的hasMoreBefore/runOpen与旧cut一致，sourceCall身份不变，但phase反映当前parent任务状态，可由running推进为error。盲目冻结旧phase或只允许一次head增长都会误判合法协议。

最终新运行`native-paged-live-final-3`的四帧全部通过，demo exit0/所属process group消失、provider线程停止、supervisor exit0后才写accepted。全部590项源码/脚本/锁输入在最终运行前冻结、结束后逐项核对；程序和原始回执存SSD `.runtime-fixtures/acceptance-refresh-20261025/`。未读私有journal，无请求body/凭证落盘，无桌面操作。

验证：demo feature626/0/0，Python全套330/0（新增12项，含错误元数据与中间cut回归），默认/feature严格Clippy、格式和Rust边界通过。两个既有独立consumer归档程序本次实际执行exit0；其默认138项源码digest与feature191项compiled输入逐项未变，SDK精确50ec49f和依赖锁未变。它们只构造/挂载及公开API类型检查，实际SEP方法由本profile另证。

独立只读review确认原控件/intent/source/安装链，提出prefix元数据检查缺口；主会话按实际公开协议区分冻结runOpen与当前sourceCall.phase，补mutation及内容回传断言后重新冻结并完整实跑。旧失败尝试及其可执行文件/日志保留：包括read必填参数与Responses续轮识别、宿主第二main槽、固定三次Earlier假设、错误分批发布时单prefix假设；不将它们改写为通过。

前实现4f2c02b的精确push40139176236/PR40139178635 CI已核对：root/UI default/feature witness/独立consumer通过，仅既有TekesUI #53严格嵌套reading失败；这是该旧head的CI，不能转授本次待提交源码。本次提交后另检查精确CI。固定1026格矩阵仍绑定91129d7，不自动升级。

未验收：真实三次以上Earlier、前插可见消息、阅读锚点/位置、桌面/物理IME/OS AX、完整Views/ProcessContainer和性能。SDK跨宿主checkpoint #42与嵌套reading #53仍开放，Kernel#86不豁免，不改Kernel/Swift/TekesUI，不关闭#17/#18/#23/#27，不合并/tag。共享target继续用于后续验证，未新建target、删除缓存0字节；保留失败证据及验收程序。
