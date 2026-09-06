const query = new URLSearchParams(window.location.search);

if (query.has('library-runtime')) {
  void import('./library-runtime/main');
} else {
  void import('./studio-main');
}
