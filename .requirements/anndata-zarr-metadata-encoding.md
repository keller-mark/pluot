# AnnData Storage Metadata Types

- Represent the encoding-type/encoding-version metadata found in AnnData's Zarr-based storage format as a discriminated set of types, one per known encoding.
- Encoding-type and encoding-version fields must only accept their exact expected string values — any other value should fail to parse.
- Must support multiple version variants of the same encoding-type (not just the latest).
- Coverage should be validated against real example files and cross-checked against the upstream AnnData file-format specification.
