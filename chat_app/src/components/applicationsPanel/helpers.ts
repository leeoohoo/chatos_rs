import type { Application } from '../../types';

import type { ApplicationFormData } from './types';

const DEFAULT_APPLICATION_FORM_DATA: ApplicationFormData = {
  name: '',
  url: '',
  iconUrl: '',
};

export const getDefaultApplicationFormData = (): ApplicationFormData => ({
  ...DEFAULT_APPLICATION_FORM_DATA,
});

export const toApplicationFormData = (app: Application): ApplicationFormData => ({
  name: app.name || '',
  url: app.url || '',
  iconUrl: app.iconUrl || '',
});

export const canSubmitApplicationForm = (formData: ApplicationFormData): boolean => {
  return formData.name.trim().length > 0;
};
