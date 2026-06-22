import { useState, type FC } from 'react';
import { useI18n } from '../../../i18n/I18nProvider';

interface PasswordFieldProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  autoComplete?: string;
}

const EyeIcon: FC<{ className?: string }> = ({ className = 'w-4 h-4' }) => (
  <svg
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.8"
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
    aria-hidden="true"
  >
    <path d="M2.5 12s3.5-6 9.5-6 9.5 6 9.5 6-3.5 6-9.5 6-9.5-6-9.5-6Z" />
    <circle cx="12" cy="12" r="3" />
  </svg>
);

const EyeOffIcon: FC<{ className?: string }> = ({ className = 'w-4 h-4' }) => (
  <svg
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.8"
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
    aria-hidden="true"
  >
    <path d="m3 3 18 18" />
    <path d="M10.6 10.7A3 3 0 0 0 12 15a3 3 0 0 0 2.3-1.1" />
    <path d="M9.9 5.1A10.8 10.8 0 0 1 12 5c6 0 9.5 7 9.5 7a16.7 16.7 0 0 1-3.2 4.2" />
    <path d="M6.7 6.7C4.1 8.4 2.5 12 2.5 12s3.5 7 9.5 7c1.5 0 2.9-.4 4.1-1" />
  </svg>
);

export const PasswordField: FC<PasswordFieldProps> = ({
  value,
  onChange,
  placeholder,
  autoComplete,
}) => {
  const { t } = useI18n();
  const [visible, setVisible] = useState(false);
  const visibilityLabel = visible
    ? t('remoteConnection.password.hide')
    : t('remoteConnection.password.show');

  return (
    <div className="mt-1 relative">
      <input
        type={visible ? 'text' : 'password'}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="w-full px-3 py-2 pr-10 rounded border border-border bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
        placeholder={placeholder}
        autoComplete={autoComplete}
      />
      <button
        type="button"
        onClick={() => setVisible((current) => !current)}
        className="absolute inset-y-0 right-0 flex items-center px-3 text-muted-foreground hover:text-foreground focus:outline-none"
        aria-label={visibilityLabel}
        title={visibilityLabel}
      >
        {visible ? <EyeOffIcon /> : <EyeIcon />}
      </button>
    </div>
  );
};
