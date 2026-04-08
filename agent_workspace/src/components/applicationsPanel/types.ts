import type { FormEvent } from 'react';

import type { Application } from '../../types';

export interface ApplicationsPanelProps {
  isOpen?: boolean;
  onClose?: () => void;
  manageOnly?: boolean;
  title?: string;
  layout?: 'embedded' | 'modal';
  onApplicationSelect?: (app: Application) => void;
}

export interface ApplicationPanelStore {
  applications: Application[];
  loadApplications?: () => Promise<void>;
  createApplication?: (name: string, url: string, iconUrl?: string) => Promise<void>;
  updateApplication?: (id: string, updates: Partial<Application>) => Promise<void>;
  deleteApplication?: (id: string) => Promise<void>;
}

export interface ApplicationFormData {
  name: string;
  url: string;
  iconUrl: string;
}

export interface ApplicationsManageViewProps {
  applications: Application[];
  showAddForm: boolean;
  editingId: string | null;
  formData: ApplicationFormData;
  compact?: boolean;
  onToggleForm: () => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => Promise<void>;
  onCancel: () => void;
  onFormDataChange: (patch: Partial<ApplicationFormData>) => void;
  onEdit: (app: Application) => void;
  onDelete: (id: string) => Promise<void>;
}

export interface ApplicationsBrowseViewProps {
  applications: Application[];
  compact?: boolean;
  onApplicationSelect: (app: Application) => void;
  onSwitchToManageMode: () => void;
}
