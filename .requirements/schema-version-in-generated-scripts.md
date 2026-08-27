# Schema Version in Generated Scripts

- A schema-version parameter must be supported across every generated-script output target (command line, JS package, React component, Python/R functions and widgets).
- When the caller doesn't specify a schema version, the generated script should embed the current library version rather than leaving it unset.
- Generated scripts should include a comment explaining how to run them.
