# #25：真实子会话 running / failure 来源

只改AgentKit，关联#25/#27和PR#10。ClientKit c289bc9、Tekes8858ccd只读，公开精确TekesUI50ec49f不变；桌面仍不跑。

## 同步与来源

验收专用acceptance-source-witness内增加waitSourceRunOpen，默认feature关闭仍unsupported。先订阅再检查原runtime，Ready且同connectionEpoch，只等指定child原run_open=true；可选inputText再要求原TranscriptSnapshot中已有同目的地非provisional authored User。15秒有界，断开/换epoch/超时失败，不自动创建或改会话。

只对新owned child运行受控loopback请求，provider用Event保持请求；同帧原父Summary running、原child run_open和当前authored marker验证通过后才release HTTP400。终态再核对同child/父/tool原位置、同epoch、更新revision和原failed事实。provider日志/父tool.phase不能当子状态。Pi在running时可能保留本请求placement=dispatching队列项；只接受与当前child/marker完全配对的该项，不填成empty，也不放行其他排队工作。failed阶段必须无队列。Pi在waitSettled之后另通过原公开childRead等待派发gate退场，明确验证pendingInputs=0/running=false/settled=true，再捕获原failed帧；不调用取消或改写queue来制造终态。

## 已确认的Pi缺口及最小修复

真实Pi原NativeConversation的当前run有assistant.stopReason=error及匹配settled marker，但库存blank=true/status缺省。旧thread_activity只看Journal，Summary于是notStarted；不通过改写blank/status掩盖问题。ClientKit的PITranscriptProjection在run内累积failed/aborted，取消优先；SummaryProviders消费子session状态。

在既有thread_activity增加私有、只读、无历史hydrate的Native终态事实选择器。仅ready/无journal error、正确session/generation、非streaming/run_open的原快照可用。显式run必须匹配最近started/settled ID，后续User/Assistant或新marker不能复活旧失败；同run内明确aborted优先error。无marker的旧方言仅在最新authored User之后查明确error/aborted。截断缺边界、未知/正常stop、仅errorMessage不生成终态成功。Native已存在时不回退旧Journal错误。

优先级仍为waiting→running→queued→显式raw terminal→当前历史terminal→blank/idle/unknown。不改原库存/#29分类事实、pointer授权、Turn/Transcript状态机、adapter或Kernel。这里的run内累计规则对齐ClientKit；不是宣称其余Transcript细节已全部对齐。

## 验证范围

先补红/绿回归，覆盖旧失败后新输入/started、同run error后正常assistant仍失败、取消、active优先级、源身份/readiness/截断/未知状态；原数据不能改写。受控Pi/DSH真实同child running→failed与原SummaryView挂载分别记录。默认和feature检查、适用UI/普通demo/新独立公开consumer、严格Clippy/边界/矩阵与收据拒绝测试串行复用SSDtarget。

原始失败/新见证、输入源码/程序hash/运行依赖、正常退出记录保留。初次诊断程序未在覆盖前归档，只有原SHA/日志/源码重建记录，不能声称全部历史程序完整；后续runner在启动前归档或核对已有同SHA归档，归档不执行，不建立应用身份。

只证明原publication与其内存输入一致，不证明服务端多流事务一致、对整个producer伪造的抵抗或实际glyph/桌面/IME/OS AX。#25/#27不因此整体关闭；#53严格门不变，不合并/tag/发布。实际结果另记。
