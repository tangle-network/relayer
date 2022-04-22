/**
 * takes a promise to run or timeout after a certain amount of time.
 */
export function timeout<T>(promise: Promise<T>, timeout: number): Promise<T> {
  const timeoutPromise = new Promise<T>((_, reject) => {
    setTimeout(() => {
      reject(new Error(`Timed out after ${timeout} ms`));
    }, timeout);
  });

  return Promise.race([timeoutPromise, promise]);
}
