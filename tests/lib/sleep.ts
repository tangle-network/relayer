import { setTimeout } from 'timers/promises';
/**
 * Sleep for a given number of milliseconds.
 */
export function sleep(ms: number): Promise<void> {
  return setTimeout(ms);
}
