# 异步准备后的原 owner 派发权限

归属 #16/#23/#27，关联 #15/#28、PR #10。对照 ClientKit 的提交捕获/失败恢复与既有 SessionRuntime call/call_at_epoch 区分；不改变稳定主 Run 在完成重连后继续原显式提交的语义，不按 transport epoch 一律拒绝。inline 图片的既有专属 epoch policy 保持。

受控 SEP fixture 中已实际复现：原 Task 指向原 ChildPaneBinding 的可继续子 Run，在 host.prepare 完成后的 subagents.list 等待期间，Task pointer 退休；旧实现仍发 prompt。最初用普通 Task Run 等待 child catalog 的测试只超时，不作为因果证据；`owner-red-2.log` 才是旧 owner 实际派发导致的红。新测试保留原 child/controller，以便检查恢复的原草稿；不通过退休后的 Missing pane 读取伪造可写状态。

窄修为 SubmissionPreparation 增加默认无副作用同步 validate_dispatch。ScopedPreparation 使用原 binding 的 require_controls，继续委派宿主检查。检查放在所有业务前置查询之后：子 Run 重用原 runtime 的 parent/child、file 限制和新 catalog 权限判断，删除 controller 的重复目录查询；普通 prompt/command 与子 prompt 都完成公开能力读取后，等待原 bounded command channel 容量，再同步复核 owner、mark_dispatched 和 enqueue。期间没有新的 await；拒绝会释放 permit。

新增 call_optional_with_validation、submit_with_validation、prompt_child_with_validation 是已有公开方法的可选本地校验变体；原方法传默认成功检查，保留能力门禁和原 RPC 身份。没有重跑可能有副作用的 host.prepare、没有第二个状态机或自动重试。这里的线性化点是本地 command 入队；已入队后的 owner 关闭不撤回已接受请求，不声称与远端执行/响应原子。

验证：原 Task owner 在 catalog 等待中退休后零 prompt、历史 tail 不变、原 child 草稿保留；稳定主 Run 在 prepare 等待中完成重连，切换并编辑 B 后仍只对原 A/RPC 首发一次，B 草稿保留；新连接移除 prompt 能力时零 prompt并恢复原草稿。另有真实 Tokio bounded channel 回归，验证背压等待结束才检查 owner、拒绝释放容量、检查仅一次及关闭 channel 不执行检查。

本轮 Root404/0/0、UI784/0/1（既有忽略），两边 all-targets 严格 Clippy通过；独立公开 consumer已在现有fixture/同一共享target实际编译运行通过；新程序/CI结果另写原收据。新 API和文档不引入 Swift/Kernel/TekesUI 修改，SDK pin不变，桌面/系统IME/OS AX未运行。来源刷新矩阵与本生产修复分开提交和绑定；旧矩阵/旧程序的成功不自动升级到新交付。

原证据保存在 SSD `.runtime-fixtures/acceptance-refresh-20261025/`。所有 Cargo 复用 shared-target；未新建 target、未删除缓存。完整 #16/#23/#27 与 #53、Kernel#86等原门槛保持开放。
