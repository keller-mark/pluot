// Reference: https://manzt.sh/assert.ts
/**
 * Error thrown when an assertion fails.
 *
 * @copyright Trevor Manz 2026
 * @license MIT
 * @see {@link https://github.com/manzt/manzt/blob/bcf6b0/utils/assert.ts}
 */
export class AssertionError extends Error {
	/** @param message The error message. */
	constructor(message: string) {
		super(message);
		this.name = "AssertionError";
	}
}

/**
 * Make an assertion.
 *
 * Usage
 * @example
 * ```ts
 * const value: boolean = Math.random() <= 0.5;
 * assert(value, "value is greater than than 0.5!");
 * value // true
 * ```
 *
 * @param expression - The expression to test.
 * @param msg - The optional message to display if the assertion fails.
 * @returns {asserts expression}
 * @throws an {@link Error} if `expression` is not truthy.
 *
 * @copyright Trevor Manz 2026
 * @license MIT
 * @see {@link https://github.com/manzt/manzt/blob/bcf6b0/utils/assert.ts}
 */
export function assert(expression: unknown, msg = ""): asserts expression {
	if (!expression) throw new AssertionError(msg);
}
