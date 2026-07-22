import { X } from 'lucide-react';
import type { CSSProperties } from 'react';
import type { DemoProject, ViewMode } from '../types';
import { PROJECT_STATUS_LABELS } from './constants';

const projectSourceLabel = (sourceType?: string | null) => {
  const source = String(sourceType || '').trim().toLowerCase();
  if (source === 'cloud' || source === 'harness') return '云端工作区';
  if (source === 'git' || source === 'repository') return 'Git 仓库';
  if (source === 'local' || source === 'filesystem') return '本地工作区';
  return sourceType || '未标注';
};

const projectImportLabel = (importStatus?: string | null) => {
  const status = String(importStatus || '').trim();
  if (!status) return '未提供';
  const normalized = status.toLowerCase();
  if (['ready', 'complete', 'completed', 'success', 'imported'].includes(normalized)) return '已同步';
  if (normalized.includes('import') || normalized.includes('running') || normalized.includes('sync')) return '同步中';
  if (normalized.includes('fail') || normalized.includes('error')) return '同步异常';
  return status;
};

const projectItemKindLabel = (kind: NonNullable<DemoProject['planItems']>[number]['kind']) => {
  if (kind === 'requirement') return '项目需求';
  if (kind === 'work-item') return '执行事项';
  return '项目资料';
};

const projectItemStatusLabel = (status?: string | null) => {
  const normalized = String(status || '').trim().toLowerCase();
  if (!normalized) return '已收录';
  if (normalized === 'done' || normalized === 'completed') return '已完成';
  if (normalized === 'in_progress' || normalized === 'doing' || normalized === 'running') return '进行中';
  if (normalized === 'blocked') return '阻塞';
  if (normalized === 'todo' || normalized === 'pending') return '待处理';
  return status as string;
};

export function ProjectDossierFocusLayer({
  project,
  onClose,
}: {
  project: DemoProject;
  onClose: () => void;
}) {
  const contentItems = (project.planItems?.length
    ? project.planItems
    : project.files.map((title) => ({ title, status: null, kind: 'document' as const })))
    .slice(0, 6);
  const counts = project.workItemCounts;
  const sourceLabel = projectSourceLabel(project.sourceType);

  return (
    <section className="dossier-focus-layer" aria-label={`${project.name} 项目档案`}>
      <div className="dossier-focus-desk" />
      <article
        className="dossier-focus-book"
        style={{ '--dossier-accent': project.accent } as CSSProperties}
      >
        <i className="dossier-focus-book__spine" />
        <section className="dossier-focus-page is-left">
          <div className="dossier-focus-page__corner">CHATOS / USER PROJECT</div>
          <div className="dossier-focus-stamp">USER PROJECT</div>
          <span className="dossier-focus-index">NO. {project.id.slice(0, 18).toUpperCase()}</span>
          <h1>{project.name}</h1>
          <p className="dossier-focus-subtitle">{project.subtitle}</p>
          <div className="dossier-focus-rule" />
          <p className="dossier-focus-summary">{project.summary}</p>
          <dl className="dossier-focus-status-grid">
            <div><dt>当前状态</dt><dd>{PROJECT_STATUS_LABELS[project.status]}</dd></div>
            <div><dt>项目完成度</dt><dd>{project.progress}%</dd></div>
            <div><dt>工作区来源</dt><dd>{sourceLabel}</dd></div>
          </dl>
          <div className="dossier-focus-identity">
            <span>工作区资料</span>
            <dl>
              <div><dt>项目 ID</dt><dd title={project.id}>{project.id}</dd></div>
              <div><dt>真实路径</dt><dd title={project.rootPath || '未配置'}>{project.rootPath || '未配置'}</dd></div>
              <div><dt>Git 仓库</dt><dd title={project.gitUrl || '未配置'}>{project.gitUrl || '未配置'}</dd></div>
              <div><dt>导入状态</dt><dd>{projectImportLabel(project.importStatus)}</dd></div>
            </dl>
          </div>
          <div className="dossier-focus-dates">
            <div><span>创建时间</span><b>{project.createdAt || '未记录'}</b></div>
            <div><span>最近更新</span><b>{project.updatedAtExact || project.updatedAt}</b></div>
          </div>
          <small className="dossier-focus-page-number">01</small>
        </section>

        <section className="dossier-focus-page is-right">
          <div className="dossier-focus-page__corner">PROJECT CONTENTS</div>
          <span className="dossier-focus-section-title">计划与资料 · {contentItems.length} 条</span>
          <div className="dossier-focus-files">
            {contentItems.map((item, index) => (
              <div key={`${item.title}-${index}`}>
                <span>{String(index + 1).padStart(2, '0')}</span>
                <p>
                  <b title={item.title}>{item.title}</b>
                  <small>{projectItemKindLabel(item.kind)} · {projectItemStatusLabel(item.status)}</small>
                </p>
                <i />
              </div>
            ))}
          </div>
          <div className="dossier-focus-progress">
            <div><span>PROJECT COMPLETION</span><b>{project.progress}%</b></div>
            <div><i style={{ width: `${project.progress}%` }} /></div>
            <p>
              {counts && counts.total > 0
                ? `共 ${counts.total} 项 · ${counts.done} 项完成 · ${counts.running} 项执行中 · ${counts.blocked} 项阻塞`
                : `当前收录 ${contentItems.length} 条项目计划与资料`}
            </p>
          </div>
          <div className="dossier-focus-activity">
            <span>最近活动</span>
            <div><i /><p><b>项目建立</b><small>{project.createdAt || '创建时间未记录'}</small></p></div>
            <div><i className="is-current" /><p><b>资料同步</b><small>{project.updatedAtExact || project.updatedAt} · {projectImportLabel(project.importStatus)}</small></p></div>
            <div><i className="is-accent" /><p><b>当前阶段</b><small>{PROJECT_STATUS_LABELS[project.status]} · 完成度 {project.progress}%</small></p></div>
          </div>
          <small className="dossier-focus-page-number">02</small>
        </section>
      </article>

      <button className="dossier-focus-close" type="button" onClick={onClose}>
        <X size={18} />
        <span>放回书架</span>
      </button>
    </section>
  );
}

export function SpatialModeHint({ view, projectName }: { view: ViewMode; projectName: string }) {
  const copy: Partial<Record<ViewMode, { title: string; detail: string }>> = {
    computer: { title: '电脑桌面已全屏', detail: '点击桌面图标打开应用，按 Esc 返回房间。' },
    chat: { title: 'AI 聊天已全屏', detail: '直接输入消息，按 Esc 返回电脑桌面。' },
    terminal: { title: '终端已全屏', detail: '输入命令，按 Esc 返回电脑桌面。' },
    remote: { title: '远程连接已全屏', detail: '选择设备并建立连接，按 Esc 返回电脑桌面。' },
    archive: { title: '左墙用户项目书架', detail: '每页 6 本项目档案册，超过 6 个可在书架铭牌上翻页。' },
    project: { title: `正在翻阅：${projectName}`, detail: '项目资料印在实体档案纸上，按 Esc 放回书架。' },
    projection: { title: '实时任务墙已聚焦', detail: '镜头已经靠近右侧任务画面，点击条目查看状态。' },
  };
  const current = copy[view];
  if (!current) return null;

  return (
    <div className="spatial-mode-hint">
      <b>{current.title}</b>
      <span>{current.detail}</span>
    </div>
  );
}
