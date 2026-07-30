// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const MACOS_RANGE_WRITE_SCRIPT: &str = r#"
(function () {
  ObjC.import("Foundation");
  ObjC.import("AppKit");

  const inputData = $.NSFileHandle.fileHandleWithStandardInput.readDataToEndOfFile;
  const inputText = ObjC.unwrap($.NSString.alloc.initWithDataEncoding(inputData, $.NSUTF8StringEncoding));
  const request = JSON.parse(String(inputText));
  const excel = Application("Microsoft Excel");
  if (!excel.running()) throw new Error("Microsoft Excel is not running");

  function runtimeInstance() {
    const running = $.NSRunningApplication.runningApplicationsWithBundleIdentifier("com.microsoft.Excel");
    if (running.count < 1) throw new Error("Excel process identity is unavailable");
    return String(running.objectAtIndex(0).processIdentifier);
  }

  function boundedText(raw) {
    const clean = Array.from(String(raw).replace(/[\u0000-\u001f\u007f]/g, "\ufffd"));
    return { value: clean.slice(0, 128).join(""), truncated: clean.length > 128 };
  }

  function boundedIdentityText(raw) {
    const source = String(raw);
    const lossy = /[\u0000-\u001f\u007f]/.test(source);
    const clean = Array.from(source.replace(/[\u0000-\u001f\u007f]/g, "\ufffd"));
    return {
      value: clean.slice(0, 128).join(""),
      truncated: lossy || clean.length > 128
    };
  }

  function externalFormula(formula) {
    return /\[[^\]]+\][^!]*!/i.test(formula) ||
      /(?:https?|file):\/\//i.test(formula) ||
      /\\\\/.test(formula) ||
      /[A-Za-z]:\\/.test(formula);
  }

  function safeScalar(raw) {
    if (raw === null || raw === undefined) return { value: null, truncated: false };
    if (typeof raw === "boolean") return { value: raw, truncated: false };
    if (typeof raw === "number" && Number.isFinite(raw)) return { value: raw, truncated: false };
    if (typeof raw === "string") return boundedText(raw);
    return { value: null, truncated: false };
  }

  function sheetVisibility(raw) {
    const number = Number(raw);
    if (number === -1) return "visible";
    if (number === 0) return "hidden";
    if (number === 2) return "very_hidden";
    const text = String(raw).toLowerCase();
    if (text.indexOf("very") >= 0 && text.indexOf("hidden") >= 0) return "very_hidden";
    if (text.indexOf("hidden") >= 0) return "hidden";
    if (text.indexOf("visible") >= 0) return "visible";
    return "unknown";
  }

  function selectAndVerify() {
    if (runtimeInstance() !== request.runtime_instance) throw new Error("Excel process identity changed");
    const workbooks = excel.workbooks();
    if (request.workbook_index < 1 || request.workbook_index > workbooks.length) {
      throw new Error("Excel workbook position is stale");
    }
    const workbook = workbooks[request.workbook_index - 1];
    const workbookName = String(workbook.name());
    let workbookFullName = workbookName;
    try { workbookFullName = String(workbook.fullName()); } catch (_) {}
    let readOnly = true;
    try { readOnly = Boolean(workbook.readOnly()); } catch (_) {}
    if (workbookName !== request.workbook_name ||
        workbookFullName !== request.workbook_identity_source ||
        readOnly !== request.workbook_read_only || readOnly) {
      throw new Error("Excel workbook identity or writable state is stale");
    }
    const worksheets = workbook.worksheets();
    if (request.worksheet_index < 1 || request.worksheet_index > worksheets.length) {
      throw new Error("Excel worksheet position is stale");
    }
    const worksheet = worksheets[request.worksheet_index - 1];
    let protectedContents = true;
    let visibility = "unknown";
    try { protectedContents = Boolean(worksheet.protectContents()); } catch (_) {}
    try { visibility = sheetVisibility(worksheet.visible()); } catch (_) {}
    if (String(worksheet.name()) !== request.worksheet_name ||
        protectedContents !== request.worksheet_protected || protectedContents ||
        visibility !== request.worksheet_visibility || visibility !== "visible") {
      throw new Error("Excel worksheet identity or writable state is stale");
    }
    return { workbook: workbook, worksheet: worksheet };
  }

  function exactRange(worksheet) {
    const targetRange = worksheet.ranges.byName(request.range_address);
    if (Number(targetRange.firstRowIndex()) !== request.start_row ||
        Number(targetRange.firstColumnIndex()) !== request.start_column ||
        targetRange.rows().length !== request.row_count ||
        targetRange.columns().length !== request.column_count) {
      throw new Error("Excel returned a non-exact range");
    }
    const cells = targetRange.cells();
    if (cells.length !== request.cell_count) throw new Error("Excel returned an unexpected cell count");
    return { range: targetRange, cells: cells };
  }

  function cellState(cell, index) {
    /*__CHATOS_MACOS_CELL_STATE__*/
    const status = isError ? "error" : hasFormula ? "formula" :
      ((value.value === null || value.value === "") && displayed.value === "" ? "blank" : "value");
    return {
      row_offset: Math.floor(index / request.column_count),
      column_offset: index % request.column_count,
      value: value.value,
      value_truncated: value.truncated,
      displayed_text: displayed.value,
      displayed_text_truncated: displayed.truncated,
      has_formula: hasFormula,
      formula: formula,
      formula_truncated: formulaTruncated,
      formula_hidden: formulaHidden,
      formula_external_reference: formulaExternalReference,
      number_format: numberFormat.value,
      number_format_truncated: numberFormat.truncated,
      number_format_unavailable: numberFormatUnavailable,
      is_error: isError,
      status: status
    };
  }

  function readCells(cells) {
    const states = [];
    for (let index = 0; index < cells.length; index += 1) states.push(cellState(cells[index], index));
    return states;
  }

  function sameScalar(left, right) {
    if (typeof left === "number" && typeof right === "number") return Object.is(left, right);
    return left === right;
  }

  function sameExpected(expected, actual) {
    return sameScalar(expected.value, actual.value) &&
      expected.value_truncated === actual.value_truncated &&
      expected.displayed_text === actual.displayed_text &&
      expected.displayed_text_truncated === actual.displayed_text_truncated &&
      expected.has_formula === actual.has_formula &&
      expected.formula === actual.formula &&
      expected.formula_truncated === actual.formula_truncated &&
      expected.formula_hidden === actual.formula_hidden &&
      expected.formula_external_reference === actual.formula_external_reference &&
      expected.number_format === actual.number_format &&
      expected.number_format_truncated === actual.number_format_truncated &&
      expected.number_format_unavailable === actual.number_format_unavailable &&
      expected.status === actual.status;
  }

  function sameContent(expected, actual) {
    return sameScalar(expected.value, actual.value) &&
      expected.value_truncated === actual.value_truncated &&
      expected.has_formula === actual.has_formula &&
      expected.formula === actual.formula &&
      expected.formula_truncated === actual.formula_truncated &&
      expected.formula_hidden === actual.formula_hidden &&
      expected.formula_external_reference === actual.formula_external_reference &&
      expected.status === actual.status;
  }

  function sameFormat(expected, actual) {
    return expected.number_format === actual.number_format &&
      expected.number_format_truncated === actual.number_format_truncated &&
      expected.number_format_unavailable === actual.number_format_unavailable;
  }

  function writeMatches(write, actual) {
    if (write.kind === "blank") return !actual.has_formula && actual.status === "blank" && actual.value === null;
    if (write.kind === "value") return !actual.has_formula && actual.status === "value" && sameScalar(write.value, actual.value);
    if (write.kind === "formula") {
      return actual.has_formula && !actual.formula_hidden && !actual.formula_external_reference &&
        actual.formula === write.formula;
    }
    return false;
  }

  function cellHasComment(cell) {
    try {
      const comment = cell.comment();
      if (comment !== null && comment !== undefined && String(comment).length > 0) return true;
    } catch (_) {}
    return false;
  }

  function ensureSimpleCell(cell) {
    try { if (Boolean(cell.mergeCells())) throw new Error("merged cells are not writable"); } catch (error) {
      if (String(error).indexOf("merged cells") >= 0) throw error;
    }
    try { if (Boolean(cell.hasArray())) throw new Error("array formula cells are not writable"); } catch (error) {
      if (String(error).indexOf("array formula") >= 0) throw error;
    }
    if (cellHasComment(cell)) throw new Error("commented cells are not writable");
  }

  function assignCell(cell, write) {
    if (write.kind === "blank") {
      cell.clearContents();
    } else if (write.kind === "value") {
      cell.value2 = write.value;
    } else if (write.kind === "formula") {
      try { cell.formula2 = write.formula; } catch (_) { cell.formula = write.formula; }
    } else {
      throw new Error("unsupported Excel write cell kind");
    }
  }

  function restoreCell(cell, previous) {
    if (previous.has_formula) {
      try { cell.formula2 = previous.formula; } catch (_) { cell.formula = previous.formula; }
    } else if (previous.status === "blank") {
      cell.clearContents();
    } else {
      cell.value2 = previous.value;
    }
  }

  function assignNumberFormat(cell, numberFormat) {
    cell.numberFormat = numberFormat;
  }

  function restoreNumberFormat(cell, previous) {
    if (previous.number_format_unavailable || previous.number_format_truncated ||
        previous.number_format === null || previous.number_format === undefined) {
      throw new Error("previous Excel number format is not safely restorable");
    }
    cell.numberFormat = previous.number_format;
  }

  function result(status, cells) {
    return JSON.stringify({
      schema_version: 1,
      write_status: status,
      runtime_instance: request.runtime_instance,
      workbook_index: request.workbook_index,
      workbook_name: request.workbook_name,
      worksheet_index: request.worksheet_index,
      worksheet_name: request.worksheet_name,
      range_address: request.range_address,
      start_row: request.start_row,
      start_column: request.start_column,
      row_count: request.row_count,
      column_count: request.column_count,
      cell_count: request.cell_count,
      cells: cells
    });
  }

  const selected = selectAndVerify();
  const exact = exactRange(selected.worksheet);
  if (!Array.isArray(request.expected_cells) || request.expected_cells.length !== request.cell_count) {
    throw new Error("Excel write request snapshot cell count is invalid");
  }
  if (request.mutation_kind === "content" &&
      (!Array.isArray(request.write_cells) || request.write_cells.length !== request.cell_count)) {
    throw new Error("Excel content write request cell count is invalid");
  }
  if (request.mutation_kind === "number_format" &&
      (typeof request.number_format !== "string" || request.number_format.length < 1 ||
       request.number_format.length > 128)) {
    throw new Error("Excel number format request is invalid");
  }
  if (request.mutation_kind !== "content" && request.mutation_kind !== "number_format") {
    throw new Error("Excel mutation kind is unsupported");
  }
  const before = readCells(exact.cells);
  for (let index = 0; index < exact.cells.length; index += 1) {
    ensureSimpleCell(exact.cells[index]);
    if (!sameExpected(request.expected_cells[index], before[index])) {
      throw new Error("Excel range snapshot is stale");
    }
  }
  selectAndVerify();

  let mutated = false;
  try {
    for (let index = 0; index < exact.cells.length; index += 1) {
      mutated = true;
      if (request.mutation_kind === "content") {
        assignCell(exact.cells[index], request.write_cells[index]);
      } else {
        assignNumberFormat(exact.cells[index], request.number_format);
      }
    }
    selectAndVerify();
    const written = readCells(exact.cells);
    for (let index = 0; index < written.length; index += 1) {
      if (request.mutation_kind === "content") {
        if (!writeMatches(request.write_cells[index], written[index]) ||
            !sameFormat(before[index], written[index])) {
          throw new Error("Excel content write verification failed");
        }
      } else if (!sameContent(before[index], written[index]) ||
                 written[index].number_format !== request.number_format ||
                 written[index].number_format_truncated ||
                 written[index].number_format_unavailable) {
        throw new Error("Excel number format verification failed");
      }
    }
    selectAndVerify();
    return result(request.mutation_kind === "content" ? "written" : "formatted", written);
  } catch (_) {
    if (!mutated) throw new Error("Excel write failed before mutation");
    try {
      selectAndVerify();
      for (let index = 0; index < exact.cells.length; index += 1) {
        if (request.mutation_kind === "content") restoreCell(exact.cells[index], before[index]);
        else restoreNumberFormat(exact.cells[index], before[index]);
      }
      selectAndVerify();
      const restored = readCells(exact.cells);
      for (let index = 0; index < restored.length; index += 1) {
        if (!sameExpected(request.expected_cells[index], restored[index])) {
          return result("rollback_failed", []);
        }
      }
      return result("rolled_back", restored);
    } catch (_) {
      return result("rollback_failed", []);
    }
  }
})()
"#;

pub(super) fn macos_range_write_script() -> String {
    super::script_fragments::expand_script(MACOS_RANGE_WRITE_SCRIPT)
}
