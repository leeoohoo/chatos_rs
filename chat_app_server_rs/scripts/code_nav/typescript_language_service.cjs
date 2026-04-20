#!/usr/bin/env node

const fs = require('fs');
const path = require('path');

const IGNORED_DIRECTORIES = [
  'node_modules',
  '.git',
  'dist',
  'build',
  'target',
  'out',
  'coverage',
  '.next',
  '.nuxt',
];

const MAX_LOCATIONS = 200;
const MAX_SYMBOLS = 500;

function parseArgs(argv) {
  const out = {};
  for (let i = 0; i < argv.length; i += 1) {
    const part = argv[i];
    if (!part.startsWith('--')) {
      continue;
    }
    const key = part.slice(2);
    const value = argv[i + 1];
    if (value == null || value.startsWith('--')) {
      out[key] = true;
      continue;
    }
    out[key] = value;
    i += 1;
  }
  return out;
}

function normalizePath(value) {
  return path.resolve(String(value || ''));
}

function toPosixPath(value) {
  return value.split(path.sep).join('/');
}

function ensureInsideRoot(projectRoot, candidatePath) {
  const root = normalizePath(projectRoot);
  const target = normalizePath(candidatePath);
  if (target === root) {
    return true;
  }
  return target.startsWith(`${root}${path.sep}`);
}

function loadTypeScript() {
  const candidates = [
    path.resolve(__dirname, '../../../chat_app/node_modules/typescript/lib/typescript.js'),
    path.resolve(__dirname, '../../node_modules/typescript/lib/typescript.js'),
  ];

  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) {
      return require(candidate);
    }
  }

  return require('typescript');
}

function findConfigFile(ts, projectRoot, filePath) {
  const searchBase = path.dirname(filePath);
  for (const configName of ['tsconfig.json', 'jsconfig.json']) {
    const found = ts.findConfigFile(searchBase, ts.sys.fileExists, configName);
    if (found && ensureInsideRoot(projectRoot, found)) {
      return normalizePath(found);
    }
  }
  for (const configName of ['tsconfig.json', 'jsconfig.json']) {
    const rootCandidate = path.join(projectRoot, configName);
    if (ts.sys.fileExists(rootCandidate)) {
      return normalizePath(rootCandidate);
    }
  }
  return null;
}

function buildProject(ts, projectRoot, filePath, language) {
  const configPath = findConfigFile(ts, projectRoot, filePath);
  let fileNames = [];
  let compilerOptions = {};
  let currentDirectory = projectRoot;

  if (configPath) {
    const configFile = ts.readConfigFile(configPath, ts.sys.readFile);
    if (configFile.error) {
      throw new Error(ts.flattenDiagnosticMessageText(configFile.error.messageText, '\n'));
    }
    const parsed = ts.parseJsonConfigFileContent(
      configFile.config,
      ts.sys,
      path.dirname(configPath),
      undefined,
      configPath,
    );
    fileNames = parsed.fileNames || [];
    compilerOptions = parsed.options || {};
    currentDirectory = path.dirname(configPath);
  } else {
    const extensions = ['.ts', '.tsx', '.mts', '.cts', '.js', '.jsx', '.mjs', '.cjs', '.d.ts'];
    fileNames = ts.sys.readDirectory(
      projectRoot,
      extensions,
      IGNORED_DIRECTORIES.map((name) => path.join(projectRoot, name)),
    );
    compilerOptions = {
      allowJs: true,
      checkJs: false,
      jsx: ts.JsxEmit.Preserve,
      target: ts.ScriptTarget.ES2020,
      module: ts.ModuleKind.ESNext,
      moduleResolution: ts.ModuleResolutionKind.NodeJs,
      esModuleInterop: true,
      allowSyntheticDefaultImports: true,
      resolveJsonModule: true,
      skipLibCheck: true,
      maxNodeModuleJsDepth: 2,
    };
  }

  if (language === 'javascript') {
    compilerOptions = {
      ...compilerOptions,
      allowJs: true,
      checkJs: false,
    };
  }

  if (!compilerOptions.jsx) {
    compilerOptions.jsx = ts.JsxEmit.Preserve;
  }

  const canonical = ts.sys.useCaseSensitiveFileNames ? (value) => value : (value) => value.toLowerCase();
  const seen = new Set();
  const allFiles = fileNames
    .concat([filePath])
    .map((item) => normalizePath(item))
    .filter((item) => ensureInsideRoot(projectRoot, item))
    .filter((item) => {
      const key = canonical(item);
      if (seen.has(key)) {
        return false;
      }
      seen.add(key);
      return true;
    });

  const snapshots = new Map();

  const host = {
    getCompilationSettings: () => compilerOptions,
    getScriptFileNames: () => allFiles,
    getScriptVersion: (scriptFileName) => {
      try {
        const stats = fs.statSync(scriptFileName);
        return String(stats.mtimeMs);
      } catch {
        return '0';
      }
    },
    getScriptSnapshot: (scriptFileName) => {
      const normalized = normalizePath(scriptFileName);
      if (!ts.sys.fileExists(normalized)) {
        return undefined;
      }
      if (!snapshots.has(normalized)) {
        const content = ts.sys.readFile(normalized);
        if (content == null) {
          return undefined;
        }
        snapshots.set(normalized, ts.ScriptSnapshot.fromString(content));
      }
      return snapshots.get(normalized);
    },
    getCurrentDirectory: () => currentDirectory,
    getDefaultLibFileName: (options) => ts.getDefaultLibFilePath(options),
    fileExists: ts.sys.fileExists,
    readFile: ts.sys.readFile,
    readDirectory: ts.sys.readDirectory,
    directoryExists: ts.sys.directoryExists ? ts.sys.directoryExists.bind(ts.sys) : undefined,
    getDirectories: ts.sys.getDirectories ? ts.sys.getDirectories.bind(ts.sys) : undefined,
    useCaseSensitiveFileNames: () => ts.sys.useCaseSensitiveFileNames,
    getNewLine: () => ts.sys.newLine,
    realpath: ts.sys.realpath ? ts.sys.realpath.bind(ts.sys) : undefined,
  };

  return {
    languageService: ts.createLanguageService(host, ts.createDocumentRegistry()),
    compilerOptions,
    canonical,
    filePath: normalizePath(filePath),
    projectRoot: normalizePath(projectRoot),
  };
}

function getProgramSourceFile(languageService, filePath, canonical) {
  const program = languageService.getProgram();
  if (!program) {
    throw new Error('TypeScript Program 未创建成功');
  }
  const normalized = normalizePath(filePath);
  const direct = program.getSourceFile(normalized);
  if (direct) {
    return direct;
  }
  const targetKey = canonical(normalized);
  const matched = program
    .getSourceFiles()
    .find((item) => canonical(normalizePath(item.fileName)) === targetKey);
  if (!matched) {
    throw new Error(`未能加载源文件: ${normalized}`);
  }
  return matched;
}

function getPositionFromLineAndColumn(sourceFile, line, column) {
  const safeLine = Math.max(0, Number(line || 1) - 1);
  const safeColumn = Math.max(0, Number(column || 1) - 1);
  const totalLines = sourceFile.getLineAndCharacterOfPosition(sourceFile.end).line;
  const lineIndex = Math.min(safeLine, totalLines);
  const lineStart = sourceFile.getPositionOfLineAndCharacter(lineIndex, 0);
  const nextLineStart = lineIndex < totalLines
    ? sourceFile.getPositionOfLineAndCharacter(lineIndex + 1, 0)
    : sourceFile.end;
  const lineLength = Math.max(0, nextLineStart - lineStart);
  return sourceFile.getPositionOfLineAndCharacter(lineIndex, Math.min(safeColumn, lineLength));
}

function getLinePreview(content, line) {
  const lines = content.split(/\r?\n/);
  const current = lines[Math.max(0, line - 1)] || '';
  return current.length > 400 ? current.slice(0, 400) : current;
}

function createLocation(programSourceFile, projectRoot, fileName, textSpan, preferredNameSpan) {
  const normalized = normalizePath(fileName);
  if (!ensureInsideRoot(projectRoot, normalized)) {
    return null;
  }

  const content = fs.readFileSync(normalized, 'utf8');
  const tsSource = programSourceFile(normalized, content);
  const start = textSpan?.start || 0;
  const length = Math.max(1, textSpan?.length || 1);
  const end = Math.min(tsSource.end, start + length);
  const startLoc = tsSource.getLineAndCharacterOfPosition(start);
  const endLoc = tsSource.getLineAndCharacterOfPosition(end);
  const line = startLoc.line + 1;
  const column = startLoc.character + 1;
  const nameSpan = preferredNameSpan || textSpan;

  return {
    path: normalized,
    relative_path: toPosixPath(path.relative(projectRoot, normalized)),
    line,
    column,
    end_line: endLoc.line + 1,
    end_column: endLoc.character + 1,
    preview: getLinePreview(content, line),
    score: 1,
    _sortSpanStart: nameSpan?.start || start,
  };
}

function dedupeLocations(locations) {
  const out = [];
  const seen = new Set();
  for (const item of locations) {
    if (!item) {
      continue;
    }
    const key = `${item.path}:${item.line}:${item.column}:${item.end_line}:${item.end_column}`;
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    out.push(item);
  }
  out.sort((left, right) => (
    left.relative_path.localeCompare(right.relative_path)
    || left.line - right.line
    || left.column - right.column
  ));
  return out.slice(0, MAX_LOCATIONS).map(({ _sortSpanStart, ...rest }) => rest);
}

function flattenReferences(referenceEntries) {
  const references = [];
  for (const group of referenceEntries || []) {
    for (const ref of group.references || []) {
      references.push(ref);
    }
  }
  return references;
}

function mapSymbolKind(kind) {
  const lowered = String(kind || '').toLowerCase();
  if (lowered.includes('class')) return 'class';
  if (lowered.includes('interface')) return 'interface';
  if (lowered.includes('enum')) return 'enum';
  if (lowered.includes('function')) return 'function';
  if (lowered.includes('method')) return 'method';
  if (lowered.includes('property')) return 'property';
  if (lowered.includes('member')) return 'property';
  if (lowered.includes('constructor')) return 'constructor';
  if (lowered.includes('var')) return 'variable';
  if (lowered.includes('const')) return 'constant';
  if (lowered.includes('let')) return 'variable';
  if (lowered.includes('module')) return 'module';
  if (lowered.includes('namespace')) return 'namespace';
  if (lowered.includes('type')) return 'type';
  return lowered || 'symbol';
}

function isRecursiveContainerKind(kind) {
  const lowered = String(kind || '').toLowerCase();
  return (
    lowered.includes('class')
    || lowered.includes('interface')
    || lowered.includes('module')
    || lowered.includes('namespace')
    || lowered.includes('enum')
  );
}

function shouldIncludeSymbol(kind) {
  const lowered = String(kind || '').toLowerCase();
  return !lowered.includes('alias');
}

function collectNavigationSymbols(projectRoot, sourceFileFactory, tree, currentFileName, output, depth = 0) {
  if (!tree || !Array.isArray(tree.childItems)) {
    return;
  }

  for (const child of tree.childItems) {
    if (!child || !child.text || child.text === '<global>') {
      continue;
    }
    const span = Array.isArray(child.spans) && child.spans.length > 0 ? child.spans[0] : null;
    if (span && shouldIncludeSymbol(child.kind)) {
      const location = createLocation(sourceFileFactory, projectRoot, currentFileName, span);
      if (location) {
        output.push({
          name: child.text,
          kind: mapSymbolKind(child.kind),
          line: location.line,
          column: location.column,
          end_line: location.end_line,
          end_column: location.end_column,
        });
      }
    }
    if (depth === 0 || isRecursiveContainerKind(child.kind)) {
      collectNavigationSymbols(projectRoot, sourceFileFactory, child, currentFileName, output, depth + 1);
    }
  }
}

function createSourceFileFactory(ts, languageService, canonical) {
  return (fileName, fileContent) => {
    const sourceFile = getProgramSourceFile(languageService, fileName, canonical);
    if (sourceFile) {
      return sourceFile;
    }
    return ts.createSourceFile(fileName, fileContent, ts.ScriptTarget.Latest, true);
  };
}

function createDocumentSymbol(sourceFile, name, kind, node, nameNode) {
  const start = nameNode ? nameNode.getStart(sourceFile) : node.getStart(sourceFile);
  const end = node.end;
  const startLoc = sourceFile.getLineAndCharacterOfPosition(start);
  const endLoc = sourceFile.getLineAndCharacterOfPosition(end);
  return {
    name,
    kind,
    line: startLoc.line + 1,
    column: startLoc.character + 1,
    end_line: endLoc.line + 1,
    end_column: endLoc.character + 1,
  };
}

function pushNamedDeclarationSymbol(sourceFile, output, nameNode, kind, node) {
  if (!nameNode) {
    return;
  }
  const name = nameNode.getText(sourceFile).trim();
  if (!name) {
    return;
  }
  output.push(createDocumentSymbol(sourceFile, name, kind, node, nameNode));
}

function collectClassOrInterfaceMembers(ts, sourceFile, output, members) {
  for (const member of members || []) {
    if (ts.isMethodDeclaration(member) || ts.isMethodSignature(member)) {
      pushNamedDeclarationSymbol(sourceFile, output, member.name, 'method', member);
      continue;
    }
    if (ts.isPropertyDeclaration(member) || ts.isPropertySignature(member)) {
      pushNamedDeclarationSymbol(sourceFile, output, member.name, 'property', member);
      continue;
    }
    if (ts.isGetAccessorDeclaration(member)) {
      pushNamedDeclarationSymbol(sourceFile, output, member.name, 'getter', member);
      continue;
    }
    if (ts.isSetAccessorDeclaration(member)) {
      pushNamedDeclarationSymbol(sourceFile, output, member.name, 'setter', member);
      continue;
    }
    if (ts.isConstructorDeclaration(member)) {
      output.push(createDocumentSymbol(sourceFile, 'constructor', 'constructor', member));
    }
  }
}

function collectTopLevelDocumentSymbols(ts, sourceFile, output) {
  for (const statement of sourceFile.statements) {
    if (ts.isFunctionDeclaration(statement)) {
      pushNamedDeclarationSymbol(sourceFile, output, statement.name, 'function', statement);
      continue;
    }
    if (ts.isClassDeclaration(statement)) {
      pushNamedDeclarationSymbol(sourceFile, output, statement.name, 'class', statement);
      collectClassOrInterfaceMembers(ts, sourceFile, output, statement.members);
      continue;
    }
    if (ts.isInterfaceDeclaration(statement)) {
      pushNamedDeclarationSymbol(sourceFile, output, statement.name, 'interface', statement);
      collectClassOrInterfaceMembers(ts, sourceFile, output, statement.members);
      continue;
    }
    if (ts.isEnumDeclaration(statement)) {
      pushNamedDeclarationSymbol(sourceFile, output, statement.name, 'enum', statement);
      continue;
    }
    if (ts.isTypeAliasDeclaration(statement)) {
      pushNamedDeclarationSymbol(sourceFile, output, statement.name, 'type', statement);
      continue;
    }
    if (ts.isModuleDeclaration(statement)) {
      pushNamedDeclarationSymbol(sourceFile, output, statement.name, 'module', statement);
      continue;
    }
    if (ts.isVariableStatement(statement)) {
      const declarationList = statement.declarationList;
      const isConst = (declarationList.flags & ts.NodeFlags.Const) !== 0;
      for (const declaration of declarationList.declarations) {
        if (ts.isIdentifier(declaration.name)) {
          output.push(createDocumentSymbol(
            sourceFile,
            declaration.name.text,
            isConst ? 'constant' : 'variable',
            declaration,
            declaration.name,
          ));
        }
      }
    }
  }
}

function runDefinition(ts, project) {
  const sourceFile = getProgramSourceFile(project.languageService, project.filePath, project.canonical);
  const position = getPositionFromLineAndColumn(sourceFile, project.line, project.column);
  const definitions = project.languageService.getDefinitionAtPosition(project.filePath, position) || [];
  const sourceFactory = createSourceFileFactory(ts, project.languageService, project.canonical);

  const locations = definitions.map((item) => {
    const span = item.contextSpan || item.textSpan;
    return createLocation(sourceFactory, project.projectRoot, item.fileName, span, item.textSpan);
  });

  return { locations: dedupeLocations(locations) };
}

function runReferences(ts, project) {
  const sourceFile = getProgramSourceFile(project.languageService, project.filePath, project.canonical);
  const position = getPositionFromLineAndColumn(sourceFile, project.line, project.column);
  const allReferences = flattenReferences(project.languageService.findReferences(project.filePath, position));
  const definitions = project.languageService.getDefinitionAtPosition(project.filePath, position) || [];
  const definitionKeys = new Set(definitions.map((item) => (
    `${normalizePath(item.fileName)}:${item.textSpan.start}:${item.textSpan.length}`
  )));
  const nonDefinitionReferences = allReferences.filter((item) => !definitionKeys.has(
    `${normalizePath(item.fileName)}:${item.textSpan.start}:${item.textSpan.length}`,
  ));
  const activeReferences = nonDefinitionReferences.length > 0 ? nonDefinitionReferences : allReferences;
  const sourceFactory = createSourceFileFactory(ts, project.languageService, project.canonical);

  const locations = activeReferences.map((item) => (
    createLocation(sourceFactory, project.projectRoot, item.fileName, item.textSpan)
  ));

  return { locations: dedupeLocations(locations) };
}

function runDocumentSymbols(ts, project) {
  const sourceFile = getProgramSourceFile(project.languageService, project.filePath, project.canonical);
  const symbols = [];
  collectTopLevelDocumentSymbols(ts, sourceFile, symbols);

  const seen = new Set();
  const deduped = [];
  for (const item of symbols) {
    const key = `${item.name}:${item.kind}:${item.line}:${item.column}`;
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    deduped.push(item);
  }

  deduped.sort((left, right) => (
    left.line - right.line
    || left.column - right.column
    || left.name.localeCompare(right.name)
  ));

  return { symbols: deduped.slice(0, MAX_SYMBOLS) };
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const mode = String(args.mode || '').trim();
  const language = String(args.language || '').trim() || 'typescript';
  const projectRoot = normalizePath(args['project-root']);
  const filePath = normalizePath(args.file);

  if (!mode) {
    throw new Error('mode 不能为空');
  }
  if (!fs.existsSync(projectRoot) || !fs.statSync(projectRoot).isDirectory()) {
    throw new Error(`project_root 不存在或不是目录: ${projectRoot}`);
  }
  if (!fs.existsSync(filePath) || !fs.statSync(filePath).isFile()) {
    throw new Error(`file 不存在或不是文件: ${filePath}`);
  }

  const ts = loadTypeScript();
  const project = buildProject(ts, projectRoot, filePath, language);
  project.line = Number(args.line || 1);
  project.column = Number(args.column || 1);

  let result;
  if (mode === 'definition') {
    result = runDefinition(ts, project);
  } else if (mode === 'references') {
    result = runReferences(ts, project);
  } else if (mode === 'document-symbols') {
    result = runDocumentSymbols(ts, project);
  } else {
    throw new Error(`不支持的 mode: ${mode}`);
  }

  process.stdout.write(`${JSON.stringify(result)}\n`);
}

try {
  main();
} catch (error) {
  const message = error instanceof Error ? (error.stack || error.message) : String(error);
  process.stderr.write(`${message}\n`);
  process.exit(1);
}
