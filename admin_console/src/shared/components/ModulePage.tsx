import { Space, Typography } from 'antd';

export function ModulePage({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="admin-module-page">
      <Space direction="vertical" size="large" style={{ width: '100%' }}>
        <Space direction="vertical" size={4}>
          <Typography.Title level={3} style={{ margin: 0 }}>{title}</Typography.Title>
          {description ? <Typography.Text type="secondary">{description}</Typography.Text> : null}
        </Space>
        {children}
      </Space>
    </div>
  );
}
