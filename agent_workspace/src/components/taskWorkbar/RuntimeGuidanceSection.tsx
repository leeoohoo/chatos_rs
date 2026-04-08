import {
  formatGuidanceItemTime,
  guidanceStatusStyles,
  guidanceStatusText,
} from './helpers';
import type { RuntimeGuidanceWorkbarItem } from './types';

interface RuntimeGuidanceSectionProps {
  items: RuntimeGuidanceWorkbarItem[];
}

const RuntimeGuidanceSection = ({ items }: RuntimeGuidanceSectionProps) => {
  if (items.length === 0) {
    return null;
  }

  return (
    <div className="mb-2 rounded-md border border-border bg-background px-2 py-1.5">
      <div className="mb-1 text-[11px] font-medium text-foreground">最近引导</div>
      <div className="space-y-1.5">
        {items.map((item) => {
          const timeText = formatGuidanceItemTime(item);
          const contentText = (item.content || '').trim();
          return (
            <div key={item.guidanceId} className="rounded border border-border/70 bg-card/60 px-2 py-1">
              <div className="flex items-center gap-1.5">
                <span className={`rounded px-1.5 py-0.5 text-[10px] font-medium ${guidanceStatusStyles[item.status]}`}>
                  {guidanceStatusText[item.status]}
                </span>
                {timeText ? (
                  <span className="text-[10px] text-muted-foreground">{timeText}</span>
                ) : null}
              </div>
              <div
                className="mt-0.5 break-all text-[11px] text-foreground"
                title={contentText || '引导内容为空'}
              >
                {contentText || '引导内容为空'}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};

export default RuntimeGuidanceSection;
