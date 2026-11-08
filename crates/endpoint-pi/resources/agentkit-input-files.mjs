const MAX_FILES = 8;
const MAX_FILE_BYTES = 8 * 1024 * 1024;
const MAX_TOTAL_BYTES = 16 * 1024 * 1024;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const CONTROL = /[\u0000-\u001f\u007f-\u009f]/;

const nonNegativeSafeInteger = value => Number.isSafeInteger(value) && value >= 0;

// Validate and snapshot the same-session upload_record metadata accompanying
// an input.prepare request. A missing field is the legacy no-file form.
export function snapshotInputFiles(value) {
  if (value === undefined) return Object.freeze([]);
  if (!Array.isArray(value) || value.length > MAX_FILES) throw new Error('Invalid input files');

  let totalBytes = 0;
  let previousContentIndex = -1;
  let previousImageCount = 0;
  const snapshot = value.map(item => {
    if (!item || typeof item !== 'object' || Array.isArray(item)) throw new Error('Invalid input file position');
    const { contentIndex, precedingImageCount = 0, file } = item;
    if (!nonNegativeSafeInteger(contentIndex) || !nonNegativeSafeInteger(precedingImageCount)
      || precedingImageCount > MAX_FILES || contentIndex < precedingImageCount
      || contentIndex <= previousContentIndex || precedingImageCount < previousImageCount) {
      throw new Error('Invalid input file position');
    }
    if (!file || typeof file !== 'object' || Array.isArray(file)) throw new Error('Invalid input file metadata');
    const { attachmentId, name, bytes } = file;
    if (typeof attachmentId !== 'string' || !UUID.test(attachmentId)
      || typeof name !== 'string' || name.length === 0 || name === '.' || name === '..'
      || /[\\/]/.test(name) || CONTROL.test(name)
      || !Number.isSafeInteger(bytes) || bytes < 1 || bytes > MAX_FILE_BYTES) {
      throw new Error('Invalid input file metadata');
    }
    totalBytes += bytes;
    if (totalBytes > MAX_TOTAL_BYTES) throw new Error('Input files exceed total size limit');
    previousContentIndex = contentIndex;
    previousImageCount = precedingImageCount;
    return Object.freeze({
      contentIndex,
      precedingImageCount,
      file: Object.freeze({ attachmentId, name, bytes }),
    });
  });
  return Object.freeze(snapshot);
}

// Keep old receipts byte-for-byte compatible when there are no files.
export function inputReceiptPayload(fields, files) {
  return files.length === 0 ? { ...fields } : {
    ...fields,
    files: files.map(({ contentIndex, precedingImageCount, file }) => ({
      contentIndex,
      precedingImageCount,
      file: { attachmentId: file.attachmentId, name: file.name, bytes: file.bytes },
    })),
  };
}
