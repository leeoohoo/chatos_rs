# Third-party notices

This package contains bundled JavaScript libraries, WebAssembly, and pinned native executables. The machine-readable component inventory is in `SBOM.cdx.json`; complete license texts copied from the production bundle inputs and vendor manifests are in `THIRD_PARTY_LICENSES.txt`. The fixed PDFium build also has a self-contained audit and redistribution bundle in `PDFIUM_THIRD_PARTY_NOTICES.txt`.

## Bundled npm runtime

The esbuild production input graph currently contains:

- MIT: `@hyzyla/pdfium`, `@modelcontextprotocol/sdk`, `@pdf-lib/standard-fonts`, `@pdf-lib/upng`, `ajv`, `ajv-formats`, `fast-deep-equal`, `fflate`, `json-schema-traverse`, `pdf-lib`, and `zod`
- BSD-3-Clause: `fast-uri`
- MIT AND Zlib: `pako`
- Apache-2.0: `pdfjs-dist`
- 0BSD: `tslib`
- ISC: `zod-to-json-schema`

Exact versions, package URLs, integrity hashes, and bundle paths are recorded in the SBOM.

## Vendored executables and WebAssembly

- OfficeCLI `1.0.144` — Apache-2.0. Six platform/architecture release binaries are pinned by URL, size, and SHA-256 in `vendor/officecli-v1.0.144.json`.
- `@hyzyla/pdfium` `2.1.13` wrapper — MIT, tag `v2.1.13`, commit `274cac6e238b780eb4cafc989d7a5a70ffc5772b`.
- PDFium WebAssembly — built from `pdfium-lib` release `7243`, which identifies the upstream PDFium branch as `chromium/7243`. The packaged WASM SHA-256 is `71aec412a303a0405baee21c3d6d3f30ad2033dc02444130fe476be3976e2d09`.
- `pdfium-lib` build project — MIT. The upstream PDFium license contains BSD-3-Clause and Apache-2.0 terms.
- PDFium-linked libraries and data — Abseil, AGG, fast_float, FreeType, ICU, Little CMS, OpenJPEG, zlib, libjpeg-turbo, and Foxit base font data.
- PDFium WASM toolchain/runtime code — Emscripten 4.0.10, musl libc, LLVM libc++, libc++abi, libunwind, and compiler-rt.

PDFium provenance, release archive digest, license-file hashes, and the packaged WASM hash are pinned in `vendor/pdfium-v7243.json`. Exact subcomponent versions/revisions, inclusion evidence, exclusions, required notices, and license hashes are pinned in `vendor/pdfium-third-party-v7243.json`.

Required attribution includes:

- Portions of this software are copyright © 1996-2002, 2006 The FreeType Project (https://freetype.org). All rights reserved.
- This software is based in part on the work of the Independent JPEG Group.
- Original PDFium font-data code copyright 2014 Foxit Software Inc.

The dependency inventory and redistribution notices for this exact PDFium WASM are now captured and automatically checked for drift. This engineering review is not legal advice; a publisher can still choose to obtain counsel before release, but no unresolved PDFium notice-collection task remains in the project gate.
