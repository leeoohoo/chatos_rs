import { DocumentError } from '../errors.js';
import { editOfficeArtifact } from '../office/artifact.js';

export async function manageSpreadsheetSheets(args: Record<string, unknown>): Promise<Record<string, unknown>> {
  if (typeof args.inputPath !== 'string' || typeof args.outputName !== 'string') {
    throw new DocumentError('INVALID_ARGUMENT', 'inputPath and outputName are required.');
  }
  if (!Array.isArray(args.operations) || args.operations.length < 1 || args.operations.length > 100) {
    throw new DocumentError('INVALID_ARGUMENT', 'operations must contain between 1 and 100 sheet operations.');
  }
  const result = await editOfficeArtifact({
    inputPath: args.inputPath,
    outputName: args.outputName,
    operations: args.operations
  });
  return {
    ...result,
    operation: 'spreadsheet_manage_sheets',
    appliedOperations: args.operations.length
  };
}
