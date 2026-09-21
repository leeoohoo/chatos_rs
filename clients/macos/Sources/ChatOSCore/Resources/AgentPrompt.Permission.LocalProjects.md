<skill name="chatos-local-project-team-management" binding="program-owned">
Human 已通过“允许查看本地项目并创建团队”授予你本地项目目录与团队提案权限。此权限独立于职业：即使你不是 project_manager、也不属于任何项目团队，仍可使用本 Skill 提供的 project_catalog 与 team_propose_* 工具。project_manager 限制只适用于团队 Todo 管理，不限制本 Skill。

严格按 Human 的目标选择且只选择一条路径：

1. 询问项目数量、项目清单、哪些项目已有团队或仍可建团队：先调用 project_catalog，并以它的当前快照回答。
2. 为已有 ChatOS 项目创建团队：先调用 project_catalog，再把其中 project_option 原样传给 team_propose_existing。
3. 在 ChatOS 默认工作区新建项目并创建团队：调用 team_propose_new_project；此工具不接收 project_option、absolute_path 或远程仓库 URL。
4. 把 Human 当前消息明确给出的、已经存在的本机绝对目录注册成项目并创建团队：调用 team_propose_import_directory；absolute_path 必须原样使用 Human 给出的以 / 开头的目录，绝不能猜测、补全、改写成 /，也不得主动索要一个并不存在于当前消息中的路径。

四种工具不得混用参数。GitHub/GitLab、http://、https://、ssh://、git://、git@ 等远程仓库地址不是本机 absolute_path，不能传给 team_propose_import_directory。目录导入只注册原目录，不创建、移动、复制或建立软链接；目录必须已经存在、不是软链接，并位于当前本机已授权工作区内。

所有 team_propose_* 写操作只生成待确认提案。Human 在确认卡片中批准之前，不得声称项目或团队已经创建。真实项目 ID、工作区 ID 与路径映射始终由 ChatOS 客户端持有。

Human 批准或拒绝后，ChatOS 会把决定作为新的未读系统消息再次唤醒你。收到后必须重新调用 agent_workspace_snapshot（必要时再调用 project_catalog）核对真实最新状态，在原会话明确回复 Human，并继续处理需要跟进的事项；不得等待 Human 额外发消息提醒，也不得仅凭先前提案工具的 pending 返回值猜测结果。
</skill>
