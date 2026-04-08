interface ChatInterfaceErrorBannerProps {
  error: string | null;
  onClear: () => void;
}

export default function ChatInterfaceErrorBanner({
  error,
  onClear,
}: ChatInterfaceErrorBannerProps) {
  if (!error) {
    return null;
  }

  return (
    <div className="mx-4 mt-4 p-3 bg-destructive/10 border border-destructive/20 rounded-lg">
      <div className="flex items-center justify-between">
        <p className="text-sm text-destructive">{error}</p>
        <button
          onClick={onClear}
          className="text-destructive hover:text-destructive/80 transition-colors"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>
    </div>
  );
}
