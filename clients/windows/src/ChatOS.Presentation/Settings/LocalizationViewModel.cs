using ChatOS.Core.Domain;
using ChatOS.Core.State;
using CommunityToolkit.Mvvm.ComponentModel;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Settings;

public sealed class LocalizationViewModel : ObservableObject
{
    private readonly AppPreferencesManager _preferences;
    private readonly IUiDispatcher _dispatcher;

    public LocalizationViewModel(AppPreferencesManager preferences, IUiDispatcher dispatcher)
    {
        _preferences = preferences;
        _dispatcher = dispatcher;
        _preferences.Changed += OnPreferencesChanged;
    }

    public InterfaceLanguage Language => _preferences.Current.Language;

    public string SignIn => Text("登录", "Sign in");
    public string SignInTitle => Text("登录 ChatOS", "Sign in to ChatOS");
    public string SignInDescription => Text(
        "使用你的 ChatOS 账号连接工作区。访问令牌只保存在 Windows 凭据管理器。",
        "Connect to your workspace with your ChatOS account. Access tokens are stored only in Windows Credential Manager.");
    public string Username => Text("用户名", "Username");
    public string UsernamePlaceholder => Text("请输入用户名", "Enter your username");
    public string Password => Text("密码", "Password");
    public string PasswordPlaceholder => Text("请输入密码", "Enter your password");
    public string CreateAccount => Text("创建账号", "Create account");
    public string RegisterDescription => Text("邀请测试 · 邮箱验证", "Invitation beta · Email verification");
    public string Account => Text("账号", "Account");
    public string AccountPlaceholder => "name@example.com";
    public string InvitationCode => Text("邀请码", "Invitation code");
    public string InvitationCodePlaceholder => Text("输入邀请测试码", "Enter invitation code");
    public string EmailVerificationCode => Text("邮箱验证码", "Email verification code");
    public string RegistrationVerificationCodePlaceholder => Text("6 位验证码", "6-digit code");
    public string ConfirmPassword => Text("确认密码", "Confirm password");
    public string ConfirmPasswordPlaceholder => Text("再次输入密码", "Enter password again");
    public string MinimumPasswordPlaceholder => Text("12–128 个字符", "12–128 characters");
    public string NoAccount => Text("还没有账号？", "New to ChatOS?");
    public string AlreadyHaveAccount => Text("已有账号？", "Already have an account?");
    public string BackToSignIn => Text("返回登录", "Back to sign in");
    public string LoginBrandHeadline => Text("路还很长，", "The road ahead is long,");
    public string LoginBrandMemory => Text("我记得我们走到了哪里。", "and I remember how far we’ve come.");
    public string LoginBrandTagline => Text("以岁月打磨，致敬每一份认真。", "Refined over time, in honor of every earnest effort.");
    public string Contacts => Text("联系人", "Contacts");
    public string Projects => Text("项目", "Projects");
    public string Applications => Text("应用", "Applications");
    public string InstalledPluginApplications => Text("已安装插件应用", "Installed plugin applications");
    public string Local => Text("本机", "Local");
    public string Remote => Text("远端", "Remote");
    public string Chat => Text("聊天", "Chat");
    public string ProjectDirectory => Text("项目目录", "Project directory");
    public string Files => Text("文件", "Files");
    public string Git => "Git";
    public string Run => Text("运行", "Run");
    public string Notepad => Text("记事本", "Notepad");
    public string Artifacts => Text("插件产物", "Plugin artifacts");
    public string Artifact => Text("产物", "Artifacts");
    public string Open => Text("打开", "Open");
    public string SaveAs => Text("另存为", "Save as");
    public string Refresh => Text("刷新", "Refresh");
    public string AllPluginArtifacts => Text(
        "所有当前插件会话产生的文件",
        "Files produced by all current plugin sessions");
    public string CurrentVisualSessionArtifacts => Text(
        "当前视觉会话产生的文件",
        "Files produced by the current visual session");
    public string NoPluginArtifacts => Text(
        "暂时没有可打开的插件产物",
        "No plugin artifacts are available yet");
    public string ProcessingFile => Text("正在处理文件…", "Processing file…");
    public string AiExecutionView => Text("AI 执行画面", "AI execution view");
    public string CloseCurrentView => Text("关闭当前画面", "Close current view");
    public string WaitingForPluginFrame => Text("正在等待插件画面…", "Waiting for the plugin frame…");
    public string Settings => Text("设置", "Settings");
    public string Back => Text("返回", "Back");
    public string Close => Text("关闭", "Close");
    public string SignOut => Text("退出登录", "Sign out");
    public string RefreshResources => Text("刷新资源", "Refresh resources");
    public string CreateResource => Text("新建资源", "Create resource");
    public string CreateLocalTerminal => Text("新建本机终端", "New local terminal");
    public string CreateLocalProject => Text("新建本地项目", "New local project");
    public string RenameLocalProject => Text("重命名所选项目", "Rename selected project");
    public string RemoveLocalProject => Text("移除所选项目", "Remove selected project");
    public string ProjectSettings => Text("项目设置", "Project settings");
    public string RunTargetCommand => Text("命令", "Command");
    public string RunTargetWorkingDirectory => Text("工作目录", "Working directory");
    public string RunTargetLanguage => Text("语言", "Language");
    public string RunTargetSource => Text("来源", "Source");
    public string RunTargetManifest => Text("清单", "Manifest");
    public string RunTargetEntrypoint => Text("入口", "Entrypoint");
    public string RemoveProjectFromChatOS => Text("从 ChatOS 删除项目", "Remove project from ChatOS");
    public string RemoveProjectKeepFilesNotice => Text(
        "只删除项目记录和运行状态，不会删除本机项目文件夹。",
        "Removes the project record and run state without deleting the local project folder.");
    public string DeleteProject => Text("删除项目", "Delete project");
    public string Terminal => Text("终端", "Terminal");
    public string StopTerminal => Text("停止终端", "Stop terminal");
    public string TerminalCommandPlaceholder => Text("输入命令后按 Enter", "Enter a command and press Enter");
    public string Send => Text("发送", "Send");
    public string ReasoningOn => Text("推理开", "Reasoning on");
    public string ReasoningOff => Text("推理关", "Reasoning off");
    public string ConversationRemote => Text("会话远端", "Conversation remote");
    public string Cancel => Text("取消", "Cancel");
    public string Create => Text("创建", "Create");
    public string New => Text("新建", "New");
    public string Delete => Text("删除", "Delete");
    public string Rename => Text("重命名", "Rename");
    public string Select => Text("选择", "Select");
    public string Execute => Text("执行", "Run");
    public string Status => Text("状态", "Status");
    public string Total => Text("总计", "Total");
    public string Tasks => Text("任务", "Tasks");
    public string Documents => Text("文档", "Documents");
    public string AcceptanceCriteria => Text("验收标准", "Acceptance criteria");
    public string RemoteConnections => Text("远端连接", "Remote connections");
    public string RemoteCredentialNotice => Text(
        "SSH 凭据只保存在本机 Credential Manager",
        "SSH credentials are stored only in Windows Credential Manager");
    public string SftpFiles => Text("SFTP 文件", "SFTP files");
    public string RemoteTerminal => Text("远程终端", "Remote terminal");
    public string ConnectionInformation => Text("连接信息", "Connection details");
    public string Name => Text("名称", "Name");
    public string LocalWorkspace => Text("本机工作区", "Local workspace");
    public string Host => Text("主机", "Host");
    public string Port => Text("端口", "Port");
    public string AuthenticationMethod => Text("认证方式", "Authentication");
    public string PrivateKey => Text("私钥", "Private key");
    public string PrivateKeyAndCertificate => Text("私钥 + 证书", "Private key + certificate");
    public string LocalOnlyPassword => Text("密码（仅本机）", "Password (local only)");
    public string PrivateKeyPath => Text("私钥路径", "Private key path");
    public string CertificatePath => Text("证书路径", "Certificate path");
    public string DefaultRemoteDirectory => Text("默认远端目录", "Default remote directory");
    public string HostKeyPolicy => Text("主机密钥策略", "Host key policy");
    public string StrictVerification => Text("严格校验", "Strict verification");
    public string AcceptFirstUse => Text("首次接受", "Accept first use");
    public string JumpHost => Text("跳板机", "Jump host");
    public string ConnectThroughJumpHost => Text("通过跳板机连接", "Connect through a jump host");
    public string ReuseSavedConnection => Text("复用已保存连接（可选）", "Reuse a saved connection (optional)");
    public string JumpHostAddress => Text("跳板机主机", "Jump host");
    public string JumpHostLocalOnlyPassword => Text("跳板机密码（仅本机）", "Jump host password (local only)");
    public string JumpHostPrivateKey => Text("跳板机私钥", "Jump host private key");
    public string JumpHostCertificate => Text("跳板机证书", "Jump host certificate");
    public string VerificationCodePlaceholder => Text("输入二次验证码", "Enter verification code");
    public string TestConnection => Text("测试连接", "Test connection");
    public string Save => Text("保存", "Save");
    public string ParentDirectory => Text("上一级", "Parent");
    public string NewDirectory => Text("新建目录", "New directory");
    public string Upload => Text("上传", "Upload");
    public string FilePreview => Text("文件预览", "File preview");
    public string SshVerificationRefreshPlaceholder => Text("输入 SSH 二次验证码后刷新", "Enter the SSH verification code, then refresh");
    public string RemoteVerificationRetryPlaceholder => Text("输入二次验证码后重新执行", "Enter the verification code, then run again");
    public string RemoteCommandPlaceholder => Text("输入远端命令，Enter 执行", "Enter a remote command and press Enter");
    public string RefreshDirectory => Text("刷新目录", "Refresh directory");
    public string NewFileOrFolder => Text("新建文件或文件夹", "New file or folder");
    public string NewFile => Text("新建文件", "New file");
    public string NewFolder => Text("新建文件夹", "New folder");
    public string SearchFilesAndContent => Text("搜索文件名和内容", "Search file names and content");
    public string Search => Text("搜索", "Search");
    public string SearchResults => Text("搜索结果", "Search results");
    public string ClearSearch => Text("清空搜索", "Clear search");
    public string SearchingFilesAndContent => Text("正在搜索文件和内容…", "Searching files and content…");
    public string NoFileSearchResults => Text("未找到匹配的文件或内容", "No matching files or content");
    public string EmptyProjectDirectory => Text("此项目目录为空", "This project directory is empty");
    public string ToggleDirectoryView => Text("切换目录视图", "Toggle directory view");
    public string Edit => Text("编辑", "Edit");
    public string Copy => Text("复制", "Copy");
    public string UnsavedFileChanges => Text("文件有未保存的修改", "Unsaved file changes");
    public string UnsavedFileChangesNotice => Text(
        "继续操作前，要保存当前文件的修改吗？",
        "Do you want to save the current file before continuing?");
    public string SaveAndContinue => Text("保存并继续", "Save and continue");
    public string DiscardChanges => Text("放弃修改", "Discard changes");
    public string ImagePreviewFailed => Text("无法预览这张图片", "This image could not be previewed");
    public string OpenInPet => Text("宠物中打开", "Open in Pet");
    public string PetFileWorkbench => Text("宠物文件工作台", "Pet File Workbench");
    public string RevealInExplorer => Text("在资源管理器中显示", "Show in File Explorer");
    public string OpenWithDefaultApplication => Text("使用默认应用打开", "Open with default app");
    public string MoreFileActions => Text("更多文件操作", "More file actions");
    public string Line => Text("行", "Line");
    public string Column => Text("列", "Column");
    public string FindDefinition => Text("查定义", "Go to definition");
    public string FindReferences => Text("查引用", "Find references");
    public string BinaryPreviewOnly => Text("二进制文件仅支持预览元数据", "Binary files support metadata preview only");
    public string BinaryEditorSafetyNotice => Text(
        "为了避免损坏文件，Windows 客户端不会把二进制内容放进文本编辑器。",
        "To avoid file corruption, the Windows client does not load binary content into the text editor.");
    public string SelectFileToView => Text("选择文件查看内容", "Select a file to view its contents");
    public string ReadOnlyPreviewNotice => Text("默认以只读预览打开，需要修改时再点击“编辑”。", "Files open in read-only preview by default. Choose Edit when changes are needed.");
    public string Pull => Text("拉取", "Pull");
    public string Push => Text("推送", "Push");
    public string RefreshGitStatus => Text("刷新 Git 状态", "Refresh Git status");
    public string CancelGitOperation => Text("取消当前 Git 操作", "Cancel current Git operation");
    public string NotGitRepository => Text("这个项目还不是 Git 仓库", "This project is not a Git repository yet");
    public string GitInitializationNotice => Text(
        "初始化只会创建 .git 元数据，不会删除或覆盖项目文件。默认分支名称为 main。",
        "Initialization only creates .git metadata and will not delete or overwrite project files. The default branch is main.");
    public string InitializeGitRepository => Text("初始化 Git 仓库", "Initialize Git repository");
    public string Changes => Text("修改", "Changes");
    public string StageAll => Text("全部暂存", "Stage all");
    public string UnstageAll => Text("全部取消暂存", "Unstage all");
    public string Staged => Text("暂存", "Staged");
    public string WorkingTree => Text("工作区", "Working tree");
    public string WorkingTreeDiff => Text("工作区 Diff", "Working tree diff");
    public string StagedDiff => Text("暂存 Diff", "Staged diff");
    public string Stage => Text("暂存", "Stage");
    public string Unstage => Text("取消暂存", "Unstage");
    public string NoWorkingTreeChanges => Text("工作区没有修改", "No working tree changes");
    public string CreateCommit => Text("创建提交", "Create commit");
    public string CommitMessagePlaceholder => Text("说明这次修改解决了什么", "Describe what this change accomplishes");
    public string CommitStagedChanges => Text("提交暂存修改", "Commit staged changes");
    public string DiffReadOnlyNotice => Text("Diff 只读预览；暂存和工作区内容分别查看。", "Diff is read-only; staged and working-tree changes are shown separately.");
    public string SelectChangeToViewDiff => Text("选择修改查看 Diff", "Select a change to view its diff");
    public string SplitDiffNotice => Text("同一个文件可能同时有暂存修改和工作区修改。", "A file may have both staged and working-tree changes.");
    public string NoTextDiff => Text("这部分修改没有文本 Diff。", "This change has no text diff.");
    public string GitActionStatus => Text("Git 操作状态", "Git action status");
    public string GitError => Text("Git 错误", "Git error");
    public string Branches => Text("分支", "Branches");
    public string Switch => Text("切换", "Switch");
    public string Merge => Text("合并", "Merge");
    public string Remotes => Text("远程仓库", "Remotes");
    public string Add => Text("添加", "Add");
    public string EditRemote => Text("编辑远程仓库", "Edit remote");
    public string RemoveRemote => Text("移除远程仓库", "Remove remote");
    public string FastForwardOnlyNotice => Text("拉取只允许 fast-forward，不会自动合并或覆盖本地修改。", "Pull only allows fast-forward and will not merge or overwrite local changes automatically.");
    public string RecentCommits => Text("最近提交", "Recent commits");
    public string ProjectRun => Text("项目运行", "Project run");
    public string TargetsSuffix => Text("个目标", "targets");
    public string Reanalyze => Text("重新分析", "Analyze again");
    public string RefreshRunStatus => Text("刷新运行状态", "Refresh run status");
    public string DismissNotice => Text("关闭提示", "Dismiss notice");
    public string DismissError => Text("关闭错误", "Dismiss error");
    public string FavoriteProject => Text("设为常用项目", "Favorite project");
    public string FavoriteProjectDescription => Text("开启后，点击桌面宠物即可查看这个项目的最近消息并直接发送新消息。", "When enabled, select the desktop pet to view recent messages and send new messages for this project.");
    public string RunPreflight => Text("运行预检", "Run preflight");
    public string NoBlockingRunIssues => Text("没有发现阻塞问题，可以启动当前目标。", "No blocking issues were found. The selected target can start.");
    public string RunTargets => Text("运行目标", "Run targets");
    public string NoRunTargets => Text("没有识别到运行目标。点击“重新分析”扫描项目入口和清单文件。", "No run targets were detected. Choose Analyze again to scan project entry points and manifest files.");
    public string StartNewInstance => Text("启动新实例", "Start new instance");
    public string RunInstances => Text("运行实例", "Run instances");
    public string RunInstanceDescription => Text("每次启动都会创建独立进程。运行中的实例可直接停止，结束后的实例仍保留日志供查看。", "Each start creates an independent process. Running instances can be stopped, and completed instances retain their logs.");
    public string NoRunInstances => Text("还没有运行实例。", "No run instances yet.");
    public string Stop => Text("停止", "Stop");
    public string RequiredEnvironment => Text("启动所需环境", "Required environment");
    public string RuntimeReady => Text("运行环境已准备好", "Runtime ready");
    public string RuntimeReadyDescription => Text("已找到当前运行目标所需的本机工具，通常无需修改自动选择。", "The local tools required by the current run target were found. You usually do not need to change the automatic selections.");
    public string RuntimeIssuesNeedAttention => Text("启动前还需要处理环境问题", "Runtime issues need attention");
    public string RuntimeIssuesDescription => Text("请先处理上方运行预检中报告的缺失工具或配置问题。", "Resolve the missing tools or configuration issues reported by the run preflight checks above.");
    public string AdvancedEnvironmentVariables => Text("高级设置：环境变量", "Advanced: environment variables");
    public string AdvancedEnvironmentVariablesDescription => Text("只有项目明确要求 PORT、NODE_ENV 等变量时才需要设置。", "Only configure these when the project explicitly requires variables such as PORT or NODE_ENV.");
    public string EnvironmentVariablesLocalNotice => Text("这些值只会传给这台 Windows 电脑上由 ChatOS 启动的项目进程。", "These values are only passed to project processes started by ChatOS on this Windows PC.");
    public string ExpandRunLog => Text("展开运行日志", "Expand run log");
    public string CollapseRunLog => Text("收起运行日志", "Collapse run log");
    public string LiveRunLog => Text("实时运行日志", "Live run log");
    public string NoRunLogYet => Text("进程已经启动，暂时还没有日志输出。", "The process has started, but no log output is available yet.");
    public string RefreshEverySecond => Text("每秒刷新", "Refreshes every second");
    public string ExitCode => Text("退出码", "Exit code");
    public string NoManualToolchain => Text("没有需要手动选择的工具链，当前目标可使用系统自动发现的环境。", "No toolchain requires manual selection; the selected target can use the automatically detected environment.");
    public string EnvironmentVariables => Text("环境变量", "Environment variables");
    public string AddVariable => Text("添加变量", "Add variable");
    public string NoEnvironmentVariables => Text("当前没有自定义环境变量。", "No custom environment variables.");
    public string EnvironmentVariableNamePlaceholder => Text("变量名，例如 PORT", "Variable name, for example PORT");
    public string Value => Text("值", "Value");
    public string DeleteEnvironmentVariable => Text("删除环境变量", "Delete environment variable");
    public string SaveEnvironment => Text("保存环境配置", "Save environment");
    public string ProjectConfigurationFiles => Text("项目配置文件", "Project configuration files");
    public string LocalApprovalRequired => Text("需要本机审批", "Local approval required");
    public string LocalOperationAwaitingApproval => Text("本机操作等待审批", "Local operation awaiting approval");
    public string Deny => Text("拒绝", "Deny");
    public string AllowOnce => Text("本次允许", "Allow once");
    public string AllowForSession => Text("本会话允许", "Allow for session");
    public string LowRisk => Text("低风险", "Low risk");
    public string MediumRisk => Text("需注意", "Caution");
    public string HighRisk => Text("高风险", "High risk");
    public string DefaultApprovalReason => Text("请确认是否允许执行此操作。", "Confirm whether this operation is allowed.");
    public string ApprovalContinuesTask => Text("处理后会立即继续或终止对应任务", "The related task will continue or stop immediately after this decision");
    public string QueuedApprovals(int count) => Text(
        $"当前还有 {count} 个审批排队等待处理",
        $"{count} more approval{(count == 1 ? string.Empty : "s")} waiting in the queue");
    public string RunningGuidanceAvailable => Text("执行中 · 可以继续发送引导", "Running · you can continue sending guidance");
    public string RefreshConversation => Text("刷新会话", "Refresh conversation");
    public string AiExecutionProcess => Text("AI 执行过程", "AI execution process");
    public string ViewProcess => Text("查看过程", "View process");
    public string TaskGraphLabel => Text("任务图", "Task graph");
    public string StartNewConversation => Text("开始一段新对话", "Start a new conversation");
    public string NoConversationMessages => Text("这个会话还没有消息。", "This conversation has no messages yet.");
    public string MessageComposerPlaceholder => Text("发送消息；执行中会作为追加指导", "Send a message; while running, it will be appended as guidance");
    public string AddAttachment => Text("添加附件", "Add attachment");
    public string RemoveAttachment => Text("移除附件", "Remove attachment");
    public string NoAvailableConversation => Text("这个资源还没有可用会话", "This resource has no available conversation");
    public string ConversationPreparationNotice => Text("项目会自动准备默认会话；联系人需要已有会话。", "Projects prepare a default conversation automatically; contacts require an existing conversation.");
    public string PreparingProjectConversation => Text("正在准备项目对话…", "Preparing the project conversation…");
    public string PreparingProjectConversationNotice => Text(
        "正在绑定项目目录和本机执行环境，完成后即可读取、搜索和修改项目。",
        "Binding the project directory and local execution environment. You can read, search, and modify the project when this completes.");
    public string ProjectConversationUnavailable => Text("项目对话暂时不可用", "Project conversation is unavailable");
    public string RetryProjectConversation => Text("重试项目对话", "Retry project conversation");
    public string TaskDetails => Text("任务详情", "Task details");
    public string LoadingTaskDetails => Text("正在加载任务详情", "Loading task details");
    public string LoadingTaskEvents => Text("正在加载执行过程", "Loading execution events");
    public string ApplyingTaskAction => Text("正在处理任务操作", "Applying task action");
    public string WaitingForTaskGraph => Text("正在同步任务图…", "Syncing the task graph…");
    public string EmptyTaskGraph => Text("当前消息没有任务图", "No task graph for this message");
    public string EmptyTaskGraphHint => Text("AI 尚未为这条消息创建任务节点，可稍后刷新。", "The AI has not created task nodes for this message yet. Refresh to check again.");
    public string TaskGraphLoadFailed => Text("任务图加载失败", "Task graph could not be loaded");
    public string UsingPreviousTaskGraph => Text("暂时无法更新任务图，正在显示上次结果。", "The task graph could not be updated. Showing the previous results.");
    public string TaskGraphRealtimeInterrupted => Text("实时进度连接已中断，活动任务将继续自动刷新。", "Live progress was disconnected. Active tasks will continue to refresh automatically.");
    public string AuthoritativeTaskStatus => Text("状态与运行过程保存在本机 Local Agent Host", "Status and run events are stored by the on-device Local Agent Host");
    public string FrozenTaskModel => Text("冻结的执行模型", "Frozen execution model");
    public string FrozenTaskReasoning => Text("冻结的推理等级", "Frozen reasoning level");
    public string FrozenTaskModelRevision => Text("模型配置版本", "Model configuration revision");
    public string FrozenTaskModelNotice => Text(
        "任务与重试始终使用创建时固定的模型配置；设置刷新不会改写历史任务。",
        "Tasks and retries keep the model configuration frozen at creation; settings refresh never rewrites history.");
    public string RunResult => Text("运行结果", "Run result");
    public string TaskAcceptanceResult => Text("验收结果", "Acceptance result");
    public string TaskAcceptanceStatus => Text("上报状态", "Reported status");
    public string TaskAcceptanceReason => Text("上报原因", "Reported reason");
    public string TaskAcceptanceFinalResponse => Text("最终交付原文", "Original final delivery");
    public string TaskAcceptanceCharacterCount => Text("Unicode 字符数 / 上限", "Unicode characters / limit");
    public string TaskAcceptanceWorkerNotice => Text(
        "验收依据来自执行者报告，不等同于独立审计或工具运行状态。",
        "Acceptance evidence is worker-reported; it is not an independent audit or tool status.");
    public string TaskAcceptanceReportMissing => Text(
        "本次执行已结束，但结构化验收报告缺失或无效；不能据此判定验收通过。",
        "Execution finished, but its structured acceptance report is missing or invalid; acceptance cannot be considered passed.");
    public string FrozenRequirementPlan => Text("引用的需求方案", "Referenced requirement plan");
    public string RequirementPlanVersion => Text("方案版本", "Plan revision");
    public string FrozenRequirementPlanNotice => Text(
        "这是创建任务时固定的已确认方案；历史任务和重试始终使用此版本。",
        "This confirmed plan was frozen when the task was created; history and retries always use this revision.");
    public string RequirementPlanScope => Text("冻结的任务范围", "Frozen task scope");
    public string RequirementPlanPrompt => Text("任务指令", "Task prompt");
    public string RequirementPlanExpectedOutputs => Text("预期交付", "Expected outputs");
    public string RequirementPlanAcceptanceCriteria => Text("验收标准", "Acceptance criteria");
    public string RequirementPlanHash => Text("方案正文 SHA-256", "Plan body SHA-256");
    public string NoTaskRunYet => Text("该任务尚未产生运行记录。", "This task has not started a run yet.");
    public string PrerequisiteTasks => Text("前置任务", "Prerequisite tasks");
    public string NoPrerequisiteTasks => Text("无前置任务", "No prerequisite tasks");
    public string ExecutionProcess => Text("执行过程", "Execution process");
    public string NoTaskRunEvents => Text("暂无运行事件", "No run events yet");
    public string LoadMoreRunEvents => Text("加载更多运行事件", "Load more run events");
    public string TaskActions => Text("任务操作", "Task actions");
    public string OptionalCancelReason => Text("取消原因（可选）", "Cancellation reason (optional)");
    public string CancelTask => Text("取消任务", "Cancel task");
    public string OptionalRetryInstruction => Text("重试补充说明（可选）", "Retry instructions (optional)");
    public string RetryRun => Text("重试运行", "Retry run");
    public string OperationAllowed => Text("操作已允许", "Operation allowed");
    public string OperationDenied => Text("操作已拒绝", "Operation denied");
    public string AiApproval => Text("AI 审批", "AI approval");
    public string SessionAuthorization => Text("本会话授权", "Session authorization");
    public string ApprovalPolicy => Text("审批策略", "Approval policy");
    public string ExecutionContinued => Text("已继续执行", "Execution continued");
    public string ExecutionStopped => Text("已停止执行", "Execution stopped");
    public string AdditionalQueuedApprovals(int count) => Text(
        $"另有 {count} 个审批正在等待",
        $"{count} additional approval{(count == 1 ? string.Empty : "s")} waiting");
    public string ApprovalTaskOutcome => Text("处理后对应任务会立即继续或终止", "The related task will continue or stop immediately after the decision");
    public string ViewTaskDetails => Text("查看任务详情", "View task details");
    public string Replying => Text("正在回复…", "Replying…");
    public string Remove => Text("移除", "Remove");
    public string CloseNotepad => Text("关闭记事本", "Close notepad");
    public string CurrentFolderLabel => Text("当前目录：", "Current folder:");
    public string RootFolder => Text("根目录", "Root");
    public string Folders => Text("文件夹", "Folders");
    public string SelectNote => Text("选择一条笔记", "Select a note");
    public string SelectNoteDescription => Text("从左侧打开笔记，或新建一条笔记。", "Open a note from the sidebar, or create a new one.");
    public string SearchNotes => Text("搜索标题、标签和内容", "Search titles, tags, and content");
    public string AllNotes => Text("全部笔记", "All notes");
    public string Notes => Text("笔记", "Notes");
    public string NewNote => Text("新建笔记", "New note");
    public string Title => Text("标题", "Title");
    public string Preview => Text("预览", "Preview");
    public string Split => Text("分栏", "Split");
    public string Export => Text("导出", "Export");
    public string Tags => Text("标签", "Tags");
    public string NoteContent => Text("笔记正文", "Note content");
    public string CommaSeparated => Text("用逗号分隔", "Separate with commas");
    public string SelectOrCreateNote => Text("选择或新建一篇笔记", "Select or create a note");
    public string Confirm => Text("确认", "Confirm");
    public string AccountAndSettings => Text("账号和设置", "Account and settings");
    public string General => Text("常规", "General");
    public string Connection => Text("连接", "Connection");
    public string Models => Text("模型", "Models");
    public string SyncModels => Text("同步模型", "Sync models");
    public string SaveSettings => Text("保存设置", "Save settings");
    public string LocalApprovalModel => Text("本机自动审批模型", "Local approval model");
    public string ModelRequestRetries => Text("模型请求最大重试次数", "Maximum model request retries");
    public string NoModelSelected => Text("未选择，自动审批回退用户", "Not selected; automatic approval asks the user");
    public string ClearSelection => Text("清除选择", "Clear selection");
    public string Plugins => Text("插件", "Plugins");
    public string Sandbox => Text("沙箱", "Sandbox");
    public string Approval => Text("审批", "Approval");
    public string ApprovalMode => Text("审批模式", "Approval mode");
    public string AskEveryTime => Text("需要确认", "Ask every time");
    public string AutomaticApproval => Text("自动审批", "Automatic approval");
    public string FullControl => Text("完全控制", "Full control");
    public string PendingApprovals => Text("等待处理", "Pending approvals");
    public string ApprovalHistory => Text("审批记录", "Approval history");
    public string Pet => Text("宠物", "Pet");
    public string LanguageLabel => Text("界面语言", "Interface language");
    public string ThemeLabel => Text("外观", "Appearance");
    public string FontSizeLabel => Text("字体大小", "Font size");
    public string ResetDefaults => Text("恢复默认", "Reset");
    public string SavingPreferences => Text("正在保存设置…", "Saving settings…");
    public string PreferencesSaveFailed => Text("设置未保存，请重试", "Settings were not saved. Please try again");
    public string PetEnabledLabel => Text("显示全局宠物", "Show global pet");
    public string PetSizeLabel => Text("宠物大小", "Pet size");
    public string PetResetPositionLabel => Text("恢复默认位置", "Reset position");
    public string PetShowProcessLabel => Text("展示 AI 执行过程", "Show AI execution progress");
    public string PetShowCompletionsLabel => Text("展示任务完成反馈", "Show task completion feedback");
    public string SystemTheme => Text("跟随系统", "System");
    public string LightTheme => Text("浅色", "Light");
    public string DarkTheme => Text("深色", "Dark");
    public string SimplifiedChinese => "简体中文";
    public string English => "English";
    public string SettingsDescription => Text(
        "这些设置会立即生效并保存在本机。",
        "These settings take effect immediately and are stored on this device.");

    public string Text(string chinese, string english) =>
        Language == InterfaceLanguage.English ? english : chinese;

    private async void OnPreferencesChanged(object? sender, AppPreferences preferences)
    {
        await _dispatcher.InvokeAsync(() => OnPropertyChanged(string.Empty));
    }
}
