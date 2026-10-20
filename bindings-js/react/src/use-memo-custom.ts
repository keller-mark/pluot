import {useRef} from 'react';

/**
 * A variant of `useMemo` that accepts a custom equality function to decide
 * whether the dependencies have changed, instead of `useMemo`'s built-in
 * per-element `Object.is` comparison. Needed whenever "should I recompute"
 * isn't simply "did every dependency stay reference-equal" — e.g. comparing
 * a compound dependency object by a domain rule rather than by identity.
 *
 * Implemented with a ref (there's no way to persist "the last accepted deps
 * + value" across renders without one — this is the same primitive React's
 * own `useMemo` is built on internally), but that's this hook's own
 * implementation detail, not something its callers need to think about.
 */
export function useMemoCustomComparison<T, D>(
  factory: () => T,
  dependencies: D,
  customIsEqual: (prevDeps: D, nextDeps: D) => boolean
): T {
  const ref = useRef<{deps: D; value: T} | undefined>(undefined);

  if (ref.current === undefined || !customIsEqual(ref.current.deps, dependencies)) {
    ref.current = {deps: dependencies, value: factory()};
  }

  return ref.current.value;
}
