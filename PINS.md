# GPUI dependency pins

Recorded 2026-10-21 from SPEC §10.9 (the spec records verification on this date).

| Package | Exact version | Reference |
| --- | --- | --- |
| gpui-kit | =0.8.0 | https://docs.rs/gpui-kit/0.8.0/ |
| gpui-component | =0.8.0 | https://docs.rs/gpui-component/0.8.0/ |
| gpui-base | =0.8.0 | https://docs.rs/gpui-base/0.8.0/ |
| gpui-pre | =0.3.9 | https://docs.rs/gpui-pre/0.3.9/ |

M0 reserves these exact workspace dependency declarations. The `genui-gpui`
crate is an empty scaffold: it does not yet activate GPUI dependencies, and
`Cargo.lock` only contains the active M0 dependency graph. M6 (or the separately
authorized spike) must activate the entire set together, record compilation
verification, and update the lockfile. No rendering or pin compilation is
claimed by M0. Pin changes require a dedicated change per SPEC §10.9.
