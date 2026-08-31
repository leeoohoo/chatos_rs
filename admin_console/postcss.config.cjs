const prefixSelector = require('postcss-prefix-selector');

const MODULE_PREFIXES = new Map([
  ['/modules/config-center/styles.css', '.config-center-module'],
  ['/modules/memory-engine/styles.css', '.memory-engine-module'],
  ['/modules/plugin-management/styles.css', '.plugin-management-module'],
  ['/modules/project-management/styles.css', '.project-management-module'],
  ['/modules/task-runner/styles.css', '.task-runner-module'],
]);

function modulePrefixFor(filePath = '') {
  const normalized = filePath.replaceAll('\\', '/');
  for (const [suffix, prefix] of MODULE_PREFIXES) {
    if (normalized.endsWith(suffix)) return prefix;
  }
  return null;
}

function scopeSelector(selector, prefix) {
  const normalized = selector.trim();
  if ([':root', 'html', 'body', '#root'].includes(normalized)) {
    return prefix;
  }
  const withoutDocumentRoot = normalized.replace(
    /^(?:(?:html|body|#root|:root)\s*)+/,
    '',
  );
  return `${prefix} ${withoutDocumentRoot || '*'}`;
}

module.exports = {
  plugins: [
    prefixSelector({
      prefix: '.admin-module-root',
      transform(_prefix, selector, _prefixedSelector, filePath) {
        const modulePrefix = modulePrefixFor(filePath);
        return modulePrefix ? scopeSelector(selector, modulePrefix) : selector;
      },
    }),
  ],
};
