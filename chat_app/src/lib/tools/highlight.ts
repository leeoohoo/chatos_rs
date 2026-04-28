import hljs from 'highlight.js/lib/core';

import bash from 'highlight.js/lib/languages/bash';
import c from 'highlight.js/lib/languages/c';
import cmake from 'highlight.js/lib/languages/cmake';
import cpp from 'highlight.js/lib/languages/cpp';
import csharp from 'highlight.js/lib/languages/csharp';
import css from 'highlight.js/lib/languages/css';
import dart from 'highlight.js/lib/languages/dart';
import dockerfile from 'highlight.js/lib/languages/dockerfile';
import dos from 'highlight.js/lib/languages/dos';
import go from 'highlight.js/lib/languages/go';
import gradle from 'highlight.js/lib/languages/gradle';
import graphql from 'highlight.js/lib/languages/graphql';
import ini from 'highlight.js/lib/languages/ini';
import java from 'highlight.js/lib/languages/java';
import javascript from 'highlight.js/lib/languages/javascript';
import json from 'highlight.js/lib/languages/json';
import kotlin from 'highlight.js/lib/languages/kotlin';
import less from 'highlight.js/lib/languages/less';
import lua from 'highlight.js/lib/languages/lua';
import makefile from 'highlight.js/lib/languages/makefile';
import markdown from 'highlight.js/lib/languages/markdown';
import objectivec from 'highlight.js/lib/languages/objectivec';
import php from 'highlight.js/lib/languages/php';
import plaintext from 'highlight.js/lib/languages/plaintext';
import powershell from 'highlight.js/lib/languages/powershell';
import protobuf from 'highlight.js/lib/languages/protobuf';
import python from 'highlight.js/lib/languages/python';
import r from 'highlight.js/lib/languages/r';
import ruby from 'highlight.js/lib/languages/ruby';
import rust from 'highlight.js/lib/languages/rust';
import scala from 'highlight.js/lib/languages/scala';
import scss from 'highlight.js/lib/languages/scss';
import sql from 'highlight.js/lib/languages/sql';
import swift from 'highlight.js/lib/languages/swift';
import typescript from 'highlight.js/lib/languages/typescript';
import xml from 'highlight.js/lib/languages/xml';
import yaml from 'highlight.js/lib/languages/yaml';

import type { AutoHighlightResult, HighlightResult, LanguageFn } from 'highlight.js';

const REGISTERED_LANGUAGES: Record<string, LanguageFn> = {
  bash,
  c,
  cmake,
  cpp,
  csharp,
  css,
  dart,
  dockerfile,
  dos,
  go,
  gradle,
  graphql,
  ini,
  java,
  javascript,
  json,
  kotlin,
  less,
  lua,
  makefile,
  markdown,
  objectivec,
  php,
  plaintext,
  powershell,
  protobuf,
  python,
  r,
  ruby,
  rust,
  scala,
  scss,
  sql,
  swift,
  typescript,
  xml,
  yaml,
};

for (const [languageName, grammar] of Object.entries(REGISTERED_LANGUAGES)) {
  hljs.registerLanguage(languageName, grammar);
}

hljs.registerAliases(['shell', 'sh', 'zsh'], { languageName: 'bash' });
hljs.registerAliases(['js', 'jsx', 'mjs', 'cjs'], { languageName: 'javascript' });
hljs.registerAliases(['ts', 'tsx'], { languageName: 'typescript' });
hljs.registerAliases(['yml'], { languageName: 'yaml' });
hljs.registerAliases(['html', 'htm'], { languageName: 'xml' });
hljs.registerAliases(['txt', 'text', 'log'], { languageName: 'plaintext' });
hljs.registerAliases(['properties', 'cfg', 'conf', 'env', 'toml'], { languageName: 'ini' });
hljs.registerAliases(['bat', 'cmd'], { languageName: 'dos' });
hljs.registerAliases(['cc', 'cxx', 'hpp', 'hxx', 'h'], { languageName: 'cpp' });
hljs.registerAliases(['cs'], { languageName: 'csharp' });
hljs.registerAliases(['kt', 'kts'], { languageName: 'kotlin' });
hljs.registerAliases(['rb'], { languageName: 'ruby' });
hljs.registerAliases(['py'], { languageName: 'python' });
hljs.registerAliases(['ps1'], { languageName: 'powershell' });
hljs.registerAliases(['proto'], { languageName: 'protobuf' });
hljs.registerAliases(['md'], { languageName: 'markdown' });
hljs.registerAliases(['rs'], { languageName: 'rust' });
hljs.registerAliases(['mm', 'm'], { languageName: 'objectivec' });

const AUTO_HIGHLIGHT_LANGUAGES = [
  'bash',
  'c',
  'cmake',
  'cpp',
  'csharp',
  'css',
  'dart',
  'dockerfile',
  'dos',
  'go',
  'gradle',
  'graphql',
  'ini',
  'java',
  'javascript',
  'json',
  'kotlin',
  'less',
  'lua',
  'makefile',
  'markdown',
  'objectivec',
  'php',
  'plaintext',
  'powershell',
  'protobuf',
  'python',
  'r',
  'ruby',
  'rust',
  'scala',
  'scss',
  'sql',
  'swift',
  'typescript',
  'xml',
  'yaml',
] as const;

export const isHighlightLanguageRegistered = (language: string): boolean => (
  Boolean(hljs.getLanguage(language))
);

export const highlightCodeBlock = (code: string, language: string): HighlightResult => (
  hljs.highlight(code, { language })
);

export const highlightCodeBlockAuto = (code: string): AutoHighlightResult => (
  hljs.highlightAuto(code, [...AUTO_HIGHLIGHT_LANGUAGES])
);
