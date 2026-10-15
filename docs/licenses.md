# License and upstream provenance

Original CodePet code is licensed under MIT (see LICENSE). No third-party visual assets are vendored. Original ginger-cat pose frames and care props are MIT licensed; see [asset provenance](../qml/assets/PROVENANCE.md) and the reproduced license. No upstream sounds or catalog assets are included.

The [source-art library](art-library/README.md) contains AI-generated concept/pose sheets created for CodePet; its manifest records generation/edit prompts and provenance. **Licence:** the PNGs under `docs/art-library/source/` are distributed under the repository MIT licence (LICENSE) by the project owner. They were AI-generated with OpenAI image generation and contain no third-party asset. Each PNG carries OpenAI-signed C2PA Content Credentials, which the byte-for-byte copy policy preserves. Copyright, exclusivity and distribution suitability of these outputs have not been legally reviewed.

## Rust dependencies

Direct dependencies, with versions from `Cargo.lock` and licenses read from each crate's `Cargo.toml` (and, for SQLite, the bundled source) in the local Cargo registry on 2026-10-14:

| Crate | Version | Used by | License |
| --- | --- | --- | --- |
| serde (with serde_derive) | 1.0.229 | protocol, storage | MIT OR Apache-2.0 |
| serde_json | 1.0.151 | protocol, storage, CLI | MIT OR Apache-2.0 |
| rusqlite (feature `bundled`) | 0.39.0 | storage | MIT |
| libsqlite3-sys (via rusqlite) | 0.37.0 | storage | MIT |
| SQLite amalgamation bundled in libsqlite3-sys | 3.51.3 | storage | Public domain ("The author disclaims copyright to this source code") |
| sha2 | 0.10.9 | storage | MIT OR Apache-2.0 |
| rustix (features `std`, `time`; no default features) | 1.1.5 | storage (CLOCK_MONOTONIC execution time) | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| tempfile | 3.27.0 | tests only (dev-dependency of storage and CLI) | MIT OR Apache-2.0 |

The bundled SQLite C source is compiled into the `codepet` binary, so a binary distribution includes public-domain SQLite code and the MIT-licensed rusqlite/libsqlite3-sys bindings.

Transitive crates built for Linux (from `cargo tree`, same date) declare permissive licenses: MIT OR Apache-2.0 for most (bitflags, block-buffer, cc, cfg-if, cpufeatures, crypto-common, digest, fallible-iterator, fallible-streaming-iterator, fastrand, find-msvc-tools, getrandom, hashbrown, hashlink, itoa, libc, once_cell, pkg-config, proc-macro2, quote, serde_core, shlex, smallvec, syn, typenum, vcpkg, version_check); MIT (generic-array, zmij); Zlib (foldhash); Unlicense OR MIT (memchr); Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT (linux-raw-sys; rustix is also a direct dependency, listed above); (MIT OR Apache-2.0) AND Unicode-3.0 (unicode-ident). Several are build-time or test-only (cc, pkg-config, vcpkg, proc-macro2, quote, syn, tempfile and its dependencies). This list records declared license fields; their full license texts and notice requirements have not been audited for a binary release.

## Upstream designs

The design considers Omagotchi (https://github.com/SLcode777/omagotchi) and OmaPets (https://github.com/yesmeck/OmaPets), but no code or assets are copied from either. Before adapting either repository, pin the exact commit and review code/asset licenses and attribution individually. A reference in the design does not confer redistribution rights.

Qt, QML runtimes and Omarchy/Quickshell components are external runtime dependencies, not redistributed by this repository; distribution packaging must review their applicable terms.
