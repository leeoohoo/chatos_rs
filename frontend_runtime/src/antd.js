// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export function createStandardAdminAppShell(runtime) {
  const {
    createElement,
    Layout,
    Menu,
    Space,
    Typography,
    Button,
    Outlet,
    useLocation,
    useNavigate,
    UserIcon,
    LogoutIcon,
  } = runtime;
  const { Header, Sider, Content } = Layout;

  return function StandardAdminAppShell(props) {
    const {
      brandTitle,
      brandSubtitle,
      headerSummary,
      navItems,
      currentUser,
      logoutLabel,
      logoutLoading,
      onLogout,
      headerBeforeUser,
    } = props;
    const location = useLocation();
    const navigate = useNavigate();
    const displayName = currentUser.display_name || currentUser.username;

    return createElement(
      Layout,
      { style: { minHeight: '100vh' } },
      createElement(
        Sider,
        { width: 220, theme: 'light', style: { borderRight: '1px solid #f0f0f0' } },
        createElement(
          'div',
          { style: { padding: '20px 20px 8px' } },
          createElement(
            Space,
            { direction: 'vertical', size: 0 },
            createElement(Typography.Title, { level: 4, style: { margin: 0 } }, brandTitle),
            createElement(Typography.Text, { type: 'secondary' }, brandSubtitle),
          ),
        ),
        createElement(Menu, {
          mode: 'inline',
          selectedKeys: [location.pathname],
          items: navItems,
          onClick: ({ key }) => navigate(key),
          style: { borderInlineEnd: 0 },
        }),
      ),
      createElement(
        Layout,
        null,
        createElement(
          Header,
          {
            style: {
              background: '#fff',
              borderBottom: '1px solid #f0f0f0',
              padding: '0 24px',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              gap: 16,
            },
          },
          createElement(Typography.Text, { type: 'secondary' }, headerSummary),
          createElement(
            Space,
            { size: 'middle', style: { flexShrink: 0 } },
            headerBeforeUser,
            createElement(
              Space,
              { size: 6 },
              createElement(UserIcon),
              createElement(Typography.Text, null, displayName),
              createElement(Typography.Text, { type: 'secondary' }, `(${currentUser.username})`),
            ),
            createElement(
              Button,
              {
                size: 'small',
                icon: createElement(LogoutIcon),
                loading: logoutLoading,
                onClick: onLogout,
              },
              logoutLabel,
            ),
          ),
        ),
        createElement(
          Content,
          { style: { padding: 24, background: '#f5f7fa' } },
          createElement(Outlet),
        ),
      ),
    );
  };
}
