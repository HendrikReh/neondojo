export function isChunkLoadError(error: unknown): error is Error {
  return (
    error instanceof Error &&
    /dynamically imported module|Importing a module script failed|Unable to preload CSS/i.test(
      error.message,
    )
  );
}

export class RetryableChunkError extends Error {
  constructor(
    cause: Error,
    readonly retry: () => void,
  ) {
    super(cause.message, { cause });
  }
}
