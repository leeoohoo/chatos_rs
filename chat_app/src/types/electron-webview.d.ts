declare global {
  namespace JSX {
    interface IntrinsicElements {
      webview: React.DetailedHTMLProps<
        React.HTMLAttributes<HTMLElement>,
        HTMLElement
      > & {
        src?: string;
        preload?: string;
        partition?: string;
        allowpopups?: boolean;
        webpreferences?: string;
      };
    }
  }
}

export {};
