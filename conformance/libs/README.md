# Library and documentation host goldens

`std.json` is the complete §8.8 catalog, defined by explicit §8.1 builder calls.
It is frozen test input: the runner compares the sorted, pretty JSON plus a final
LF byte for byte, and tests never regenerate it. Its hash excludes the `hash`
member and uses compact canonical JSON. The M2 hash was independently calculated
with Python's xxhash implementation, not copied from the Rust builder output.

`host.json` explicitly registers the documentation tools, ref ids and URL hosts
used by SPEC examples and `examples/*.ui`. Ordinary validation defaults to no
registered tools/refs and an empty model-URL host allowlist.
