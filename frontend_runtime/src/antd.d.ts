// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface StandardAdminShellUser {
  username: string;
  display_name?: string | null;
}

export interface StandardAdminAppShellProps {
  brandTitle: string;
  brandSubtitle: string;
  headerSummary: string;
  navItems: readonly unknown[];
  currentUser: StandardAdminShellUser;
  logoutLabel: string;
  logoutLoading?: boolean;
  onLogout: () => void;
  headerBeforeUser?: unknown;
}

export interface StandardAdminAppShellRuntime {
  createElement: (...args: any[]) => any;
  Layout: any;
  Menu: any;
  Space: any;
  Typography: any;
  Button: any;
  Outlet: any;
  useLocation: () => { pathname: string };
  useNavigate: () => (path: string) => void;
  UserIcon: any;
  LogoutIcon: any;
}

export function createStandardAdminAppShell(
  runtime: StandardAdminAppShellRuntime,
): (props: StandardAdminAppShellProps) => any;
