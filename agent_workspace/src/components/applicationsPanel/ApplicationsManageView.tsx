import { AppGridIcon } from './icons';
import type { ApplicationsManageViewProps } from './types';

const ApplicationsManageView = ({
  applications,
  showAddForm,
  editingId,
  formData,
  compact = false,
  onToggleForm,
  onSubmit,
  onCancel,
  onFormDataChange,
  onEdit,
  onDelete,
}: ApplicationsManageViewProps) => {
  return (
    <div className="space-y-4">
      <div className={compact ? 'mb-4 flex items-center justify-between' : ''}>
        <button
          type="button"
          onClick={onToggleForm}
          className={
            compact
              ? 'px-3 py-1.5 text-sm rounded bg-primary text-primary-foreground hover:opacity-90'
              : 'w-full p-4 border-2 border-dashed border-border rounded-lg hover:border-blue-500 transition-colors flex items-center justify-center space-x-2 text-muted-foreground hover:text-blue-600'
          }
        >
          <span>{showAddForm ? '取消' : compact ? '新增应用' : '新增应用'}</span>
        </button>
      </div>

      {showAddForm && (
        <form
          onSubmit={onSubmit}
          className={compact ? 'p-4 bg-muted rounded-lg space-y-3' : 'p-4 bg-muted rounded-lg space-y-4'}
        >
          <div>
            <label className={`block font-medium text-foreground mb-2 ${compact ? 'text-xs' : 'text-sm'}`}>
              名称
            </label>
            <input
              type="text"
              value={formData.name}
              onChange={(event) => onFormDataChange({ name: event.target.value })}
              className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
              placeholder={compact ? '例如：飞书' : '例如：Jira、GitHub'}
              required
            />
          </div>
          <div>
            <label className={`block font-medium text-foreground mb-2 ${compact ? 'text-xs' : 'text-sm'}`}>
              URL
            </label>
            <input
              type="text"
              value={formData.url}
              onChange={(event) => onFormDataChange({ url: event.target.value })}
              className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
              placeholder="https://app.example.com"
            />
          </div>
          <div>
            <label className={`block font-medium text-foreground mb-2 ${compact ? 'text-xs' : 'text-sm'}`}>
              图标URL
            </label>
            <input
              type="text"
              value={formData.iconUrl}
              onChange={(event) => onFormDataChange({ iconUrl: event.target.value })}
              className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
              placeholder={compact ? 'https://.../icon.png' : 'https://app.example.com/icon.png'}
            />
          </div>
          <div className="flex items-center justify-end space-x-2">
            <button
              type="button"
              className="px-3 py-2 rounded bg-muted hover:bg-accent"
              onClick={onCancel}
            >
              取消
            </button>
            <button
              type="submit"
              className={compact
                ? 'px-3 py-1.5 text-sm rounded bg-primary text-primary-foreground hover:opacity-90'
                : 'px-3 py-2 rounded bg-blue-600 text-white hover:bg-blue-700'}
            >
              {editingId ? '保存' : '创建'}
            </button>
          </div>
        </form>
      )}

      <div className="space-y-2">
        {applications.map((app) => (
          <div
            key={app.id}
            className="flex items-center justify-between p-3 rounded border border-border hover:bg-muted transition-colors"
          >
            <div className="flex items-center space-x-3">
              <div className="w-10 h-10 rounded-full bg-gradient-to-br from-blue-500 to-purple-500 flex items-center justify-center overflow-hidden shrink-0">
                {app.iconUrl ? (
                  <img src={app.iconUrl} alt={app.name} className="w-full h-full object-cover" />
                ) : (
                  <span className="text-white text-sm font-bold">
                    {app.name.charAt(0).toUpperCase()}
                  </span>
                )}
              </div>
              <div>
                <div className="text-sm font-medium text-foreground">{app.name}</div>
                {app.url && (
                  <div className="text-xs text-muted-foreground truncate max-w-md">{app.url}</div>
                )}
              </div>
            </div>
            <div className="flex items-center space-x-2">
              <button
                className={compact
                  ? 'px-2 py-1 text-xs rounded bg-background hover:bg-accent'
                  : 'px-2 py-1 text-xs bg-muted rounded hover:bg-accent'}
                onClick={() => onEdit(app)}
              >
                编辑
              </button>
              <button
                className={compact
                  ? 'px-2 py-1 text-xs rounded bg-destructive text-destructive-foreground hover:opacity-90'
                  : 'px-2 py-1 text-xs bg-destructive text-destructive-foreground rounded hover:bg-destructive/90'}
                onClick={() => void onDelete(app.id)}
              >
                删除
              </button>
            </div>
          </div>
        ))}
        {applications.length === 0 && (
          <div className="flex flex-col items-center justify-center py-10 text-center text-muted-foreground">
            <AppGridIcon className={compact ? 'w-16 h-16 text-muted-foreground/30 mb-3' : undefined} />
            <div className="text-sm">{compact ? '暂无应用，点击上方按钮添加。' : '暂无应用，点击上方按钮添加'}</div>
          </div>
        )}
      </div>
    </div>
  );
};

export default ApplicationsManageView;
