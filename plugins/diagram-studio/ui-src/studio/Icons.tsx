import type { SVGProps } from 'react';

export type IconName =
  | 'plus' | 'folder' | 'home' | 'save' | 'undo' | 'redo' | 'layout' | 'export'
  | 'sidebar' | 'inspector' | 'trash' | 'fit' | 'search' | 'chevron'
  | 'architecture' | 'flowchart' | 'swimlane' | 'topology' | 'sequence' | 'close'
  | 'user' | 'terminal' | 'mobile' | 'browser' | 'server' | 'api' | 'cloud'
  | 'database' | 'cache' | 'storage' | 'queue' | 'network' | 'shield'
  | 'container' | 'cluster' | 'monitor' | 'document' | 'note';

const paths: Record<IconName, React.ReactNode> = {
  plus: <path d="M12 5v14M5 12h14" />,
  folder: <path d="M3.5 7.5h6l2-2h9a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2h-17a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2Z" />,
  home: <><path d="m3.5 10 8.5-7 8.5 7" /><path d="M5.5 9v11h13V9M9.5 20v-6h5v6" /></>,
  save: <><path d="M5 3.5h12l3 3v14H4v-17Z" /><path d="M8 3.5v6h8v-6M8 20v-7h8v7" /></>,
  undo: <><path d="M8 8H3V3" /><path d="M3.5 7.5A9 9 0 1 1 6 18" /></>,
  redo: <><path d="M16 8h5V3" /><path d="M20.5 7.5A9 9 0 1 0 18 18" /></>,
  layout: <><rect x="3" y="4" width="7" height="6" rx="1.5" /><rect x="14" y="14" width="7" height="6" rx="1.5" /><path d="M10 7h4a3 3 0 0 1 3 3v4M14 17h-4a3 3 0 0 1-3-3v-4" /></>,
  export: <><path d="M12 3v12M7 8l5-5 5 5" /><path d="M5 13v7h14v-7" /></>,
  sidebar: <><rect x="3" y="4" width="18" height="16" rx="2.5" /><path d="M9 4v16" /></>,
  inspector: <><rect x="3" y="4" width="18" height="16" rx="2.5" /><path d="M15 4v16" /></>,
  trash: <><path d="M4 7h16M9 7V4h6v3M7 7l1 14h8l1-14" /><path d="M10 11v6M14 11v6" /></>,
  fit: <><path d="M9 4H4v5M15 4h5v5M9 20H4v-5M15 20h5v-5" /></>,
  search: <><circle cx="10.5" cy="10.5" r="6.5" /><path d="m16 16 5 5" /></>,
  chevron: <path d="m8 10 4 4 4-4" />,
  architecture: <><rect x="3" y="4" width="7" height="5" rx="1" /><rect x="14" y="15" width="7" height="5" rx="1" /><path d="M10 6.5h4a3 3 0 0 1 3 3V15M6.5 9v6h7.5" /></>,
  flowchart: <><circle cx="12" cy="4.5" r="2.5" /><path d="M12 7v3" /><path d="m12 10 4 4-4 4-4-4 4-4Z" /><path d="M12 18v3" /></>,
  swimlane: <><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M3 10h18M3 15h18M9 4v16" /></>,
  topology: <><circle cx="12" cy="5" r="3" /><circle cx="5" cy="18" r="3" /><circle cx="19" cy="18" r="3" /><path d="m10.5 7.5-4 8M13.5 7.5l4 8M8 18h8" /></>,
  sequence: <><rect x="2.5" y="3" width="6" height="4" rx="1" /><rect x="15.5" y="3" width="6" height="4" rx="1" /><path d="M5.5 7v14M18.5 7v14M7 11h10M14 8l3 3-3 3M17 17H7M10 14l-3 3 3 3" /></>,
  close: <path d="m7 7 10 10M17 7 7 17" />,
  user: <><circle cx="12" cy="8" r="3.5" /><path d="M5.5 20c.5-4 2.7-6 6.5-6s6 2 6.5 6" /></>,
  terminal: <><rect x="3" y="4" width="18" height="13" rx="2" /><path d="M8 21h8M12 17v4" /></>,
  mobile: <><rect x="7" y="2.5" width="10" height="19" rx="2.5" /><path d="M10.5 5h3M11 18.5h2" /></>,
  browser: <><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M3 8h18M6 6h.01M9 6h.01" /></>,
  server: <><rect x="4" y="3" width="16" height="7" rx="2" /><rect x="4" y="14" width="16" height="7" rx="2" /><path d="M7 6.5h.01M7 17.5h.01M11 6.5h6M11 17.5h6" /></>,
  api: <><path d="M8 4H5a2 2 0 0 0-2 2v4a2 2 0 0 0 2 2h3M16 12h3a2 2 0 0 0 2-2V6a2 2 0 0 0-2-2h-3" /><path d="M8 8h8M12 5l3 3-3 3M16 16H8M12 13l-3 3 3 3" /></>,
  cloud: <path d="M6.5 19h11a4 4 0 0 0 .5-8 6 6 0 0 0-11.5-1.5A4.8 4.8 0 0 0 6.5 19Z" />,
  database: <><ellipse cx="12" cy="5.5" rx="7" ry="3" /><path d="M5 5.5v6c0 1.7 3.1 3 7 3s7-1.3 7-3v-6M5 11.5v6c0 1.7 3.1 3 7 3s7-1.3 7-3v-6" /></>,
  cache: <><path d="m12 3-8 4 8 4 8-4-8-4Z" /><path d="m4 12 8 4 8-4M4 17l8 4 8-4" /></>,
  storage: <><path d="M4 7.5h6l2-2h8v14H4v-12Z" /><path d="M4 10h16" /></>,
  queue: <><rect x="3" y="5" width="5" height="5" rx="1" /><rect x="16" y="14" width="5" height="5" rx="1" /><path d="M8 7.5h5a3 3 0 0 1 3 3V14M13 11l3 3 3-3" /></>,
  network: <><circle cx="12" cy="5" r="2.5" /><circle cx="5" cy="19" r="2.5" /><circle cx="19" cy="19" r="2.5" /><path d="m10.8 7.2-4.6 9.6M13.2 7.2l4.6 9.6M7.5 19h9" /></>,
  shield: <path d="M12 3 5 6v5c0 4.7 2.8 8.2 7 10 4.2-1.8 7-5.3 7-10V6l-7-3Z" />,
  container: <><path d="m12 3 8 4.5v9L12 21l-8-4.5v-9L12 3Z" /><path d="m4.5 7.8 7.5 4.3 7.5-4.3M12 12v9" /></>,
  cluster: <><rect x="3" y="3" width="7" height="7" rx="1.5" /><rect x="14" y="3" width="7" height="7" rx="1.5" /><rect x="8.5" y="14" width="7" height="7" rx="1.5" /><path d="M6.5 10v2h5.5v2M17.5 10v2H12" /></>,
  monitor: <><path d="M4 18V9M9 18V5M14 18v-7M19 18V3" /><path d="M3 21h18" /></>,
  document: <><path d="M6 3h8l4 4v14H6V3Z" /><path d="M14 3v5h5M9 12h6M9 16h6" /></>,
  note: <><path d="M5 4h14v13l-4 4H5V4Z" /><path d="M15 21v-4h4M8 8h8M8 12h6" /></>
};

export function Icon({ name, ...props }: { name: IconName } & SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" {...props}>
      {paths[name]}
    </svg>
  );
}
