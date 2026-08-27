# CLI Code/Graphics Format Options

- The command-line tool must support selecting a code output format and a graphics output format as separate options.
- If a code format is given, output should be a rendering script in that code format; otherwise output should be a rendered graphic.
- Graphics format must be required when a code format is specified; when no code format is specified, an explicit graphics format overrides the format inferred from the output file's extension.
- Python and R bindings must support the same code-format selection, implemented as a plain synchronous call rather than wrapped in async/await machinery it doesn't need.
