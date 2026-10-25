import { useEffect, useMemo, useState } from "react";
import { useProjectGitStore } from "../application/projectGitStore";
import {
  parseProjectGitDiff,
  projectGitFileName,
  projectGitParentPath,
  stagedProjectGitChanges,
  visibleGitDecorations,
  workingTreeProjectGitChanges,
  type ProjectGitBranch,
  type ProjectGitChange,
  type ProjectGitCommit,
  type ProjectGitDiff,
  type ProjectGitRemote,
} from "../domain/projectGit";
import type { ProjectGitPort } from "../ports/ProjectGitPort";

export function ProjectGitView({
  ownerUserId,
  projectId,
  port,
  onOpenFile,
}: {
  ownerUserId: string;
  projectId: string;
  port: ProjectGitPort;
  onOpenFile(path: string): void;
}) {
  const state = useProjectGitStore();
  const [createBranchOpen, setCreateBranchOpen] = useState(false);
  const [mergeBranch, setMergeBranch] = useState<ProjectGitBranch | null>(null);
  const [remoteEditor, setRemoteEditor] = useState<{ remote: ProjectGitRemote | null } | null>(null);

  useEffect(() => {
    void useProjectGitStore.getState().activate(port, ownerUserId, projectId);
  }, [ownerUserId, port, projectId]);

  useEffect(() => () => useProjectGitStore.getState().reset(), [ownerUserId, projectId]);

  if (state.isLoading && !state.snapshot.isRepository) {
    return <GitEmpty icon={<span className="mini-spinner" />} title="正在读取 Git 状态…" />;
  }
  if (!state.snapshot.isRepository) {
    return (
      <div className="project-git project-git--empty" aria-label="项目 Git">
        {state.errorMessage && <GitError message={state.errorMessage} onDismiss={state.dismissError} />}
        <GitEmpty
          icon="⑂"
          title="尚未启用 Git"
          description="可以在当前项目目录创建 Git 仓库。"
          action={(
            <button
              className="project-git-primary-button"
              disabled={state.isPerformingOperation}
              onClick={() => void state.initializeRepository(port)}
              type="button"
            >初始化仓库</button>
          )}
        />
      </div>
    );
  }

  const staged = stagedProjectGitChanges(state.snapshot);
  const working = workingTreeProjectGitChanges(state.snapshot);
  const branchTitle = state.snapshot.currentBranch
    ?? (state.snapshot.detachedCommit ? `HEAD ${state.snapshot.detachedCommit}` : "尚无提交");
  const canCommit = state.commitMessage.trim().length > 0
    && staged.length > 0
    && !state.isPerformingOperation;

  return (
    <div className="project-git" aria-label="项目 Git">
      <aside className="project-git__sidebar">
        <GitToolbar
          branchTitle={branchTitle}
          branches={state.snapshot.branches}
          busy={state.isPerformingOperation}
          isLoading={state.isLoading}
          remotes={state.snapshot.remotes}
          upstream={state.snapshot.upstream}
          onCreateBranch={() => setCreateBranchOpen(true)}
          onEditRemote={(remote) => setRemoteEditor({ remote })}
          onMerge={setMergeBranch}
          onPull={() => state.snapshot.remotes.length
            ? void state.pull(port)
            : setRemoteEditor({ remote: null })}
          onPush={() => state.snapshot.remotes.length
            ? void state.push(port)
            : setRemoteEditor({ remote: null })}
          onRefresh={() => void state.load(port)}
          onSwitch={(branch) => void state.switchBranch(port, branch)}
        />
        {state.errorMessage && <GitError message={state.errorMessage} onDismiss={state.dismissError} />}
        <div className="project-git__scroll">
          <section className="project-git-commit">
            <textarea
              aria-label="Git 提交说明"
              disabled={state.isPerformingOperation}
              onChange={(event) => state.setCommitMessage(event.target.value)}
              onKeyDown={(event) => {
                if ((event.metaKey || event.ctrlKey) && event.key === "Enter" && canCommit) {
                  event.preventDefault();
                  void state.commit(port);
                }
              }}
              placeholder="提交说明"
              rows={2}
              value={state.commitMessage}
            />
            <button disabled={!canCommit} onClick={() => void state.commit(port)} type="button">
              {staged.length ? `✓ 提交 ${staged.length} 个文件` : "请先暂存修改"}
            </button>
            {(state.snapshot.aheadCount > 0 || state.snapshot.behindCount > 0) && (
              <div className="project-git-sync-counts">
                {state.snapshot.aheadCount > 0 && <span>↑ 领先 {state.snapshot.aheadCount}</span>}
                {state.snapshot.behindCount > 0 && <span>↓ 落后 {state.snapshot.behindCount}</span>}
              </div>
            )}
          </section>
          <GitChangeList
            busy={state.isPerformingOperation}
            changes={staged}
            staged
            title="已暂存"
            onOpenDiff={(change) => void state.showDiff(port, change, true)}
            onToggle={(change) => void state.unstage(port, change)}
            onToggleAll={() => void state.unstageAll(port)}
          />
          <GitChangeList
            busy={state.isPerformingOperation}
            changes={working}
            staged={false}
            title="更改"
            onOpenDiff={(change) => void state.showDiff(port, change, false)}
            onToggle={(change) => void state.stage(port, change)}
            onToggleAll={() => void state.stageAll(port)}
          />
          <GitHistory commits={state.snapshot.commits} />
        </div>
        {state.isPerformingOperation && (
          <div className="project-git__busy" role="status"><span className="mini-spinner" />正在执行 Git 操作…</div>
        )}
      </aside>
      <section className="project-git__detail">
        {state.isDiffLoading && !state.selectedDiff ? (
          <GitEmpty icon={<span className="mini-spinner" />} title="正在读取差异…" />
        ) : state.selectedDiff ? (
          <GitDiffView
            diff={state.selectedDiff}
            onClose={state.clearDiff}
            onOpenFile={() => onOpenFile(state.selectedDiff!.path)}
          />
        ) : (
          <div className="project-directory__welcome project-git__file-empty">
            <span className="project-git__file-empty-icon" aria-hidden="true"><span>▤</span><b>⌕</b></span>
            <h2>选择一个文件</h2>
            <p>从左侧项目目录中选择文件以预览或编辑。</p>
          </div>
        )}
      </section>
      {createBranchOpen && (
        <CreateBranchSheet
          busy={state.isPerformingOperation}
          onCancel={() => setCreateBranchOpen(false)}
          onCreate={(name, switchToBranch) => {
            setCreateBranchOpen(false);
            void state.createBranch(port, name, switchToBranch);
          }}
        />
      )}
      {mergeBranch && (
        <MergeBranchSheet
          branch={mergeBranch.name}
          busy={state.isPerformingOperation}
          currentBranch={state.snapshot.currentBranch ?? "当前分支"}
          onCancel={() => setMergeBranch(null)}
          onMerge={() => {
            const branch = mergeBranch.name;
            setMergeBranch(null);
            void state.mergeBranch(port, branch);
          }}
        />
      )}
      {remoteEditor && (
        <RemoteEditorSheet
          busy={state.isPerformingOperation}
          remote={remoteEditor.remote}
          onCancel={() => setRemoteEditor(null)}
          onRemove={(name) => {
            setRemoteEditor(null);
            void state.removeRemote(port, name);
          }}
          onSave={(originalName, name, url) => {
            setRemoteEditor(null);
            void state.saveRemote(port, originalName, name, url);
          }}
        />
      )}
    </div>
  );
}

function GitToolbar({
  branchTitle,
  branches,
  busy,
  isLoading,
  remotes,
  upstream,
  onCreateBranch,
  onEditRemote,
  onMerge,
  onPull,
  onPush,
  onRefresh,
  onSwitch,
}: {
  branchTitle: string;
  branches: ProjectGitBranch[];
  busy: boolean;
  isLoading: boolean;
  remotes: ProjectGitRemote[];
  upstream: string | null;
  onCreateBranch(): void;
  onEditRemote(remote: ProjectGitRemote | null): void;
  onMerge(branch: ProjectGitBranch): void;
  onPull(): void;
  onPush(): void;
  onRefresh(): void;
  onSwitch(branch: string): void;
}) {
  return (
    <header className="project-git-toolbar">
      <label title="切换分支">
        <span aria-hidden="true">⑂</span>
        <select
          aria-label="切换 Git 分支"
          disabled={busy}
          onChange={(event) => onSwitch(event.target.value)}
          value={branches.find((branch) => branch.isCurrent)?.name ?? ""}
        >
          {!branches.some((branch) => branch.isCurrent) && <option value="">{branchTitle}</option>}
          {branches.map((branch) => <option key={branch.name} value={branch.name}>{branch.name}</option>)}
        </select>
      </label>
      <button aria-label="新建 Git 分支" disabled={busy} onClick={onCreateBranch} title="新建分支" type="button">＋</button>
      <label className="project-git-toolbar__icon-select" title="合并分支到当前分支">
        <span aria-hidden="true">⑂</span>
        <select
          aria-label="合并 Git 分支"
          disabled={busy || branches.filter((branch) => !branch.isCurrent).length === 0}
          onChange={(event) => {
            const branch = branches.find((candidate) => candidate.name === event.target.value);
            event.target.value = "";
            if (branch) onMerge(branch);
          }}
          value=""
        >
          <option value="">合并</option>
          {branches.filter((branch) => !branch.isCurrent).map((branch) => (
            <option key={branch.name} value={branch.name}>{branch.name}</option>
          ))}
        </select>
      </label>
      <button aria-label="拉取 Git 更新" disabled={busy} onClick={onPull} title={remotes.length ? "拉取远程更新（仅快进）" : "先配置远端"} type="button">↓</button>
      <button aria-label="推送 Git 更新" disabled={busy} onClick={onPush} title={remotes.length ? (upstream ? "推送当前分支" : "发布当前分支") : "先配置远端"} type="button">↑</button>
      <label className="project-git-toolbar__icon-select" title="管理远端">
        <span aria-hidden="true">⌁</span>
        <select
          aria-label="管理 Git 远端"
          disabled={busy}
          onChange={(event) => {
            const remote = remotes.find((candidate) => candidate.name === event.target.value) ?? null;
            event.target.value = "";
            onEditRemote(remote);
          }}
          value=""
        >
          <option value="">远端</option>
          <option value="__new__">添加远端…</option>
          {remotes.map((remote) => <option key={remote.name} value={remote.name}>{remote.name}</option>)}
        </select>
      </label>
      <button aria-label="刷新 Git 状态" disabled={busy || isLoading} onClick={onRefresh} title="刷新 Git 状态" type="button">↻</button>
    </header>
  );
}

function GitChangeList({
  busy,
  changes,
  staged,
  title,
  onOpenDiff,
  onToggle,
  onToggleAll,
}: {
  busy: boolean;
  changes: ProjectGitChange[];
  staged: boolean;
  title: string;
  onOpenDiff(change: ProjectGitChange): void;
  onToggle(change: ProjectGitChange): void;
  onToggleAll(): void;
}) {
  const [expanded, setExpanded] = useState(true);
  return (
    <section className="project-git-change-list">
      <header>
        <button aria-label={`${expanded ? "折叠" : "展开"}${title}`} onClick={() => setExpanded(!expanded)} type="button">{expanded ? "⌄" : "›"}</button>
        <strong>{title}</strong><span>{changes.length}</span>
        <button
          aria-label={staged ? "全部取消暂存" : "全部暂存"}
          disabled={busy || changes.length === 0}
          onClick={onToggleAll}
          title={staged ? "全部取消暂存" : "全部暂存"}
          type="button"
        >{staged ? "−" : "+"}</button>
      </header>
      {expanded && (changes.length ? changes.map((change) => (
        <GitChangeRow
          busy={busy}
          change={change}
          key={`${change.path}:${change.indexStatus}:${change.workTreeStatus}:${staged}`}
          staged={staged}
          onOpenDiff={() => onOpenDiff(change)}
          onToggle={() => onToggle(change)}
        />
      )) : <p className="project-git-empty-row">{staged ? "暂存区为空" : "工作区没有未暂存修改"}</p>)}
    </section>
  );
}

function GitChangeRow({
  busy,
  change,
  staged,
  onOpenDiff,
  onToggle,
}: {
  busy: boolean;
  change: ProjectGitChange;
  staged: boolean;
  onOpenDiff(): void;
  onToggle(): void;
}) {
  const parent = projectGitParentPath(change.path);
  const status = ({ modified: "M", added: "A", deleted: "D", renamed: "R", copied: "C", untracked: "U", conflicted: "!", typeChanged: "T" } as const)[change.kind];
  const icon = ({ modified: "▧", added: "+", deleted: "−", renamed: "→", copied: "▣", untracked: "+", conflicted: "!", typeChanged: "↻" } as const)[change.kind];
  return (
    <div className="project-git-change-row" data-kind={change.kind}>
      <button aria-label={`${projectGitFileName(change.path)} ${staged ? "暂存区" : "工作区"}差异`} onClick={onOpenDiff} type="button">
        <span className="project-git-change-row__icon" aria-hidden="true">{icon}</span>
        <span><strong>{projectGitFileName(change.path)}</strong>{parent && <small>{parent}</small>}</span>
        <b>{status}</b>
      </button>
      <button
        aria-label={`${staged ? "取消暂存" : "暂存"} ${change.path}`}
        disabled={busy}
        onClick={onToggle}
        title={staged ? "取消暂存" : "暂存"}
        type="button"
      >{staged ? "⊖" : "⊕"}</button>
    </div>
  );
}

function GitHistory({ commits }: { commits: ProjectGitCommit[] }) {
  const [expanded, setExpanded] = useState(true);
  return (
    <section className="project-git-history">
      <button className="project-git-history__header" onClick={() => setExpanded(!expanded)} type="button">
        <span>{expanded ? "⌄" : "›"}</span><strong>提交历史</strong><small>{commits.length}</small>
      </button>
      {expanded && (commits.length ? commits.map((commit, index) => (
        <GitCommitRow commit={commit} first={index === 0} key={commit.id} last={index === commits.length - 1} />
      )) : <p className="project-git-empty-row">尚无提交历史</p>)}
    </section>
  );
}

function GitCommitRow({ commit, first, last }: { commit: ProjectGitCommit; first: boolean; last: boolean }) {
  const decorations = visibleGitDecorations(commit);
  return (
    <article className="project-git-commit-row">
      <span className={`project-git-commit-row__graph ${first ? "is-first" : ""} ${last ? "is-last" : ""} ${commit.parentIds.length > 1 ? "is-merge" : ""}`} />
      <div>
        <strong>{commit.subject || "无提交说明"}</strong>
        {decorations.length > 0 && <div className="project-git-decorations">{decorations.map((item) => <span key={item}>{item}</span>)}</div>}
        <small><code>{commit.shortId}</code> {commit.author}{commit.authoredAt ? ` · ${formatGitDate(commit.authoredAt)}` : ""}</small>
      </div>
    </article>
  );
}

function GitDiffView({ diff, onClose, onOpenFile }: { diff: ProjectGitDiff; onClose(): void; onOpenFile(): void }) {
  const lines = useMemo(() => parseProjectGitDiff(diff.content), [diff.content]);
  return (
    <article className="project-git-diff">
      <header>
        <span aria-hidden="true">±</span>
        <div><strong>{projectGitFileName(diff.path)}</strong><small>{diff.isStaged ? "暂存区变更" : "工作区变更"}</small></div>
        <button onClick={onOpenFile} type="button">打开文件</button>
        <button aria-label="关闭 Git 差异" onClick={onClose} type="button">×</button>
      </header>
      {lines.length === 0 ? (
        <GitEmpty icon="▧" title="没有可显示的文本差异" description="文件可能只有元数据变化，或者内容差异已经不存在。" />
      ) : (
        <div className="project-git-diff__lines" aria-label={`${projectGitFileName(diff.path)} Git 差异`}>
          {lines.map((line) => (
            <div className={`project-git-diff-line project-git-diff-line--${line.kind}`} key={line.id}>
              <span>{line.oldLine ?? ""}</span><span>{line.newLine ?? ""}</span><b>{line.marker}</b><code>{line.content || " "}</code>
            </div>
          ))}
        </div>
      )}
    </article>
  );
}

function GitEmpty({ icon, title, description, action }: { icon: React.ReactNode; title: string; description?: string; action?: React.ReactNode }) {
  return (
    <div className="project-git-empty">
      <span aria-hidden="true">{icon}</span><h2>{title}</h2>{description && <p>{description}</p>}{action}
    </div>
  );
}

function GitError({ message, onDismiss }: { message: string; onDismiss(): void }) {
  return <div className="project-git-error" role="alert"><span>{message}</span><button aria-label="关闭 Git 错误" onClick={onDismiss} type="button">×</button></div>;
}

function CreateBranchSheet({ busy, onCancel, onCreate }: { busy: boolean; onCancel(): void; onCreate(name: string, switchToBranch: boolean): void }) {
  const [name, setName] = useState("");
  const [switchToBranch, setSwitchToBranch] = useState(true);
  return (
    <GitSheet title="新建分支" icon="⑂" onCancel={onCancel}>
      <label className="project-git-field"><span>分支名称</span><input aria-label="Git 分支名称" autoFocus onChange={(event) => setName(event.target.value)} placeholder="例如 feature/git-workbench" value={name} /></label>
      <label className="project-git-toggle"><input checked={switchToBranch} onChange={(event) => setSwitchToBranch(event.target.checked)} type="checkbox" />创建后立即切换到新分支</label>
      <footer><button disabled={busy} onClick={onCancel} type="button">取消</button><button className="primary-button" disabled={busy || !name.trim()} onClick={() => onCreate(name.trim(), switchToBranch)} type="button">创建</button></footer>
    </GitSheet>
  );
}

function MergeBranchSheet({ branch, busy, currentBranch, onCancel, onMerge }: { branch: string; busy: boolean; currentBranch: string; onCancel(): void; onMerge(): void }) {
  return (
    <GitSheet title="合并分支" icon="⑂" onCancel={onCancel}>
      <p>将“{branch}”合并到“{currentBranch}”。</p>
      <p className="project-git-sheet__note">ⓘ 如果存在冲突，工作台会保留冲突文件供你处理，不会自动覆盖内容。</p>
      <footer><button disabled={busy} onClick={onCancel} type="button">取消</button><button className="primary-button" disabled={busy} onClick={onMerge} type="button">开始合并</button></footer>
    </GitSheet>
  );
}

function RemoteEditorSheet({ busy, remote, onCancel, onRemove, onSave }: { busy: boolean; remote: ProjectGitRemote | null; onCancel(): void; onRemove(name: string): void; onSave(originalName: string | null, name: string, url: string | null): void }) {
  const [name, setName] = useState(remote?.name ?? "origin");
  const [url, setUrl] = useState(remote?.url ?? "");
  const [confirmRemove, setConfirmRemove] = useState(false);
  return (
    <GitSheet title={remote ? "编辑远端仓库" : "添加远端仓库"} icon="⌁" onCancel={onCancel}>
      <label className="project-git-field"><span>名称</span><input aria-label="Git 远端名称" onChange={(event) => setName(event.target.value)} value={name} /></label>
      <label className="project-git-field"><span>仓库地址</span><input aria-label="Git 远端地址" onChange={(event) => setUrl(event.target.value)} placeholder="https://… 或 git@…" value={url} /></label>
      {remote?.urlIsRedacted && url === remote.url && <p className="project-git-sheet__note">凭据或本机路径已由原生层隐藏；保持此字段不变会保留原始地址。</p>}
      <footer>
        {remote && <button className="danger-button" disabled={busy} onClick={() => setConfirmRemove(true)} type="button">删除远端</button>}
        <span />
        <button disabled={busy} onClick={onCancel} type="button">取消</button>
        <button
          className="primary-button"
          disabled={busy || !name.trim() || (!remote && !url.trim())}
          onClick={() => onSave(
            remote?.name ?? null,
            name.trim(),
            remote?.urlIsRedacted && url === remote.url ? null : url.trim(),
          )}
          type="button"
        >保存</button>
      </footer>
      {confirmRemove && (
        <div className="project-git-confirm" role="alertdialog" aria-label={`删除远端“${remote?.name}”？`}>
          <strong>删除远端“{remote?.name}”？</strong><p>只会删除本地 Git 配置，不会删除远端服务器上的仓库或分支。</p>
          <div><button onClick={() => setConfirmRemove(false)} type="button">取消</button><button className="danger-button" onClick={() => remote && onRemove(remote.name)} type="button">删除远端</button></div>
        </div>
      )}
    </GitSheet>
  );
}

function GitSheet({ title, icon, onCancel, children }: { title: string; icon: string; onCancel(): void; children: React.ReactNode }) {
  return (
    <div className="project-git-sheet-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget) onCancel(); }}>
      <section aria-label={title} aria-modal="true" className="project-git-sheet" role="dialog">
        <header><span aria-hidden="true">{icon}</span><h2>{title}</h2></header>{children}
      </section>
    </div>
  );
}

function formatGitDate(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}
