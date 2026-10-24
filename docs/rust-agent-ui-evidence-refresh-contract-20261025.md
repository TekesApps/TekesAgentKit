# 当前验收矩阵与来源范围更新

归属 #27/#28，关联 #15/#16/#22/#23，PR #10。保留20261024矩阵原快照；新矩阵明确固定功能基线，未来提交不自动继承其结果。生产修改与元数据刷新分别记录；不把本次归档/检查器工作写成新的协议执行。

矩阵v2保留原15需求，并补transport_recovery、image_payload_roundtrip、mixed_attachment_order、busy_queue_submission_identity。新增protocol_integration层，区分无窗口真实协议与离屏组件、桌面、系统IME、OS AX和ProcessContainer；共19×3×2×9=1026格。不同程序/提交的图片、mixed与busy结果仍是独立历史观察，Native busy失败继续引用Kernel#86，不合成三端当前全通过。普通构造consumer不满足整个行为格。

每项证据显式声明harnesses、topology、requirements、layers；当前passed/failed来源须匹配矩阵基线，带headSha的CI记录须匹配sourceCommit。历史观察保留各自来源。端点专属证据不能替另一端通过，协议证据不能提升为UI、IME或容器结果。共享组件单测明确列三端，仅表示共享代码覆盖。

新增可跟踪来源绑定：使用原SSD commit-binding、冻结manifest、归档程序和公开原始收据逐项复核Git后写小JSON，给出实际sourceCommit、源码文件哈希、原报告路径/SHA及程序/收据SHA。原报告中的baseline仅是此前工作基线，不当作编译来源；不改历史报告来配合检查器。默认CI核验受控报告及其链接SHA，--verify-local额外核验原程序、原始收据和Git文件来源；不新编译、不新建target。

Composer并行核对遵循既有契约：稳定主Run的尚未发出显式提交可在恢复后按原scope首次发送，不能无依据改成一律epoch拒绝。重点验证异步准备后的open_session/child catalog窗口中原owner撤销是否仍能发出；若实际复现，增加无副作用的最后派发前authority复核，不能重复host.prepare或新建状态机。inline图片已有专属epoch校验保持不变。桌面仍不运行。
