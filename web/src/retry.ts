import { CATALOG } from './errors';
export interface RetryPolicy { auto: boolean; maxAttempts: number; backoffMs: number[] }
export function sleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    if (signal?.aborted) {
      reject(new DOMException('Aborted', 'AbortError'));
      return;
    }
    const onAbort = () => {
      clearTimeout(timer);
      reject(new DOMException('Aborted', 'AbortError'));
    };
    const timer = setTimeout(() => {
      signal?.removeEventListener('abort', onAbort);
      resolve();
    }, ms);
    signal?.addEventListener('abort', onAbort, { once: true });
  });
}
export async function withRetry<T>(fn: () => Promise<T>, policy: RetryPolicy, opts?: { signal?: AbortSignal; onRetry?: (attempt: number, delayMs: number) => void; shouldRetry?: (e: unknown) => boolean }): Promise<T> {
  for (let attempt = 0; ; attempt++) {
    try {
      return await fn();
    } catch (e) {
      if (!policy.auto || attempt >= policy.maxAttempts || opts?.signal?.aborted || (opts?.shouldRetry && !opts.shouldRetry(e))) {
        throw e;
      }
      const delay = policy.backoffMs.length ? policy.backoffMs[Math.min(attempt, policy.backoffMs.length - 1)] : 0;
      opts?.onRetry?.(attempt + 1, delay);
      await sleep(delay, opts?.signal);
    }
  }
}
export function policyFor(code: import('./errors').ErrorCode): RetryPolicy {
  return CATALOG[code].retry;
}
