import { AppGridIcon } from './icons';
import type { ApplicationsBrowseViewProps } from './types';

const ApplicationsBrowseView = ({
  applications,
  compact = false,
  onApplicationSelect,
  onSwitchToManageMode,
}: ApplicationsBrowseViewProps) => {
  const gridClassName = compact
    ? 'grid grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-4'
    : 'grid grid-cols-4 md:grid-cols-5 lg:grid-cols-6 gap-6';

  return (
    <div className={gridClassName}>
      {applications.map((app) => (
        <div key={app.id} className="relative group/item">
          <button
            className="w-full flex flex-col items-center space-y-2 p-2 rounded-lg transition-all hover:bg-muted"
            onClick={() => onApplicationSelect(app)}
            title={app.url || ''}
          >
            <div
              className={`relative rounded-full flex items-center justify-center overflow-hidden transition-all bg-gradient-to-br from-blue-500/20 to-purple-500/20 group-hover/item:scale-105 ${
                compact ? 'w-14 h-14' : 'w-16 h-16'
              }`}
            >
              {app.iconUrl ? (
                <img src={app.iconUrl} alt={app.name} className="w-full h-full object-cover" />
              ) : (
                <div className="w-full h-full bg-gradient-to-br from-blue-500 to-purple-500 flex items-center justify-center">
                  <span className={`text-white font-bold ${compact ? 'text-lg' : 'text-xl'}`}>
                    {app.name.charAt(0).toUpperCase()}
                  </span>
                </div>
              )}
            </div>
            <div className="text-xs font-medium text-foreground text-center truncate w-full px-1">
              {app.name}
            </div>
            {compact && app.url && (
              <div className="text-[10px] text-muted-foreground truncate max-w-full">{app.url}</div>
            )}
          </button>
        </div>
      ))}
      {applications.length === 0 && (
        <div className="col-span-full flex flex-col items-center justify-center py-10 text-center">
          <AppGridIcon className={compact ? 'w-16 h-16 text-muted-foreground/30 mb-3' : 'w-20 h-20 text-muted-foreground/30 mb-4'} />
          <div className="text-sm text-muted-foreground mb-2">暂无应用</div>
          <button
            onClick={onSwitchToManageMode}
            className="text-sm text-blue-500 hover:text-blue-600 underline"
          >
            {compact ? '切换到管理模式以添加应用' : '点击切换到管理模式添加应用'}
          </button>
        </div>
      )}
    </div>
  );
};

export default ApplicationsBrowseView;
