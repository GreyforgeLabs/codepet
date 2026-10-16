# License and upstream provenance

Original CodePet code is Copyright (c) 2026 Greyforge Labs and licensed under MIT (see LICENSE). No third-party visual assets are vendored: no asset pack, download or trace from another project is included. No upstream sounds or catalog assets are included.

**Companion art shipped at runtime (v0.2).** The 15 companion packs under `qml/assets/pets/` (WebP frames, thumbnails and manifests) are distributed under the MIT licence in [`qml/assets/pets/LICENSE`](../qml/assets/pets/LICENSE) by Greyforge Labs. The frames are AI-generated: GPT Image 2 (OpenAI image generation) via the Codex CLI image tool on the project owner's ChatGPT plan, with each species' sheets anchored on its PR #7 source-art sheet. Copyright and distribution suitability of these outputs have not been legally reviewed. The generation route, every source-sheet and frame SHA-256, and each extraction override are recorded in [`qml/assets/pets/PROVENANCE.md`](../qml/assets/pets/PROVENANCE.md). The source sheets themselves are not in Git; they are the release asset [`codepet-art-sources-v0.2.tar`](https://github.com/GreyforgeLabs/codepet/releases/download/v0.2.1/codepet-art-sources-v0.2.tar), under the same licence. `_placeholder.webp` and `_memorial.webp` are drawn by `tools/petart/shared_art.py` (original, MIT). The extraction tooling in `tools/petart/` (Python with Pillow, numpy and scipy) runs only at development time; none of it, and none of those libraries, ships in or is loaded by the app.

**Legacy art.** The original v0.1 ginger-cat SVG pose frames and care props under `qml/assets/` are MIT licensed by Greyforge Labs; see [asset provenance](../qml/assets/PROVENANCE.md) and the reproduced license.

The [source-art library](art-library/README.md) contains AI-generated concept/pose sheets created for CodePet; its manifest records generation/edit prompts and provenance. **Licence:** the PNGs under `docs/art-library/source/` are distributed under the repository MIT licence (LICENSE) by Greyforge Labs. They were AI-generated with OpenAI image generation and contain no third-party asset. Each PNG carries OpenAI-signed C2PA Content Credentials, which the byte-for-byte copy policy preserves. Copyright, exclusivity and distribution suitability of these outputs have not been legally reviewed.

## Rust dependencies

Direct dependencies, with versions from `Cargo.lock` and licenses read from each crate's `Cargo.toml` (and, for SQLite, the bundled source) in the local Cargo registry on 2026-10-15:

| Crate | Version | Used by | License |
| --- | --- | --- | --- |
| serde (with serde_derive) | 1.0.229 | protocol, storage | MIT OR Apache-2.0 |
| serde_json | 1.0.151 | protocol, storage, CLI; core tests only (dev-dependency) | MIT OR Apache-2.0 |
| rusqlite (feature `bundled`) | 0.39.0 | storage | MIT |
| libsqlite3-sys (via rusqlite) | 0.37.0 | storage | MIT |
| SQLite amalgamation bundled in libsqlite3-sys | 3.51.3 | storage | Public domain ("The author disclaims copyright to this source code") |
| sha2 | 0.10.9 | storage | MIT OR Apache-2.0 |
| rustix (features `std`, `time`, `fs`, `process`; no default features) | 1.1.5 | storage (CLOCK_MONOTONIC execution time; `O_NOFOLLOW` opens of the state-dir sidecars; `PR_SET_DUMPABLE` for debug-build fault injection) | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| tempfile | 3.27.0 | tests only (dev-dependency of storage and CLI) | MIT OR Apache-2.0 |

The bundled SQLite C source is compiled into the `codepet` binary, so a binary distribution includes public-domain SQLite code and the MIT-licensed rusqlite/libsqlite3-sys bindings.

Transitive crates built for Linux (from `cargo tree`, same date) declare permissive licenses: MIT OR Apache-2.0 for most (bitflags, block-buffer, cc, cfg-if, cpufeatures, crypto-common, digest, fallible-iterator, fallible-streaming-iterator, fastrand, find-msvc-tools, getrandom, hashbrown, hashlink, itoa, libc, once_cell, pkg-config, proc-macro2, quote, serde_core, shlex, smallvec, syn, typenum, vcpkg, version_check); MIT (generic-array, zmij); Zlib (foldhash); Unlicense OR MIT (memchr); Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT (linux-raw-sys; rustix is also a direct dependency, listed above); (MIT OR Apache-2.0) AND Unicode-3.0 (unicode-ident). Several are build-time or test-only (cc, pkg-config, vcpkg, proc-macro2, quote, syn, tempfile and its dependencies). The crates actually linked into the `codepet` binary, with their full licence texts and the SQLite public-domain dedication, are in [`THIRD-PARTY-LICENSES.md`](../THIRD-PARTY-LICENSES.md), which ships in the release archive. It is generated from `cargo metadata` by `tools/release/third_party_licenses.py`; `tools/release/package.sh` refuses to package when it is out of date.

## Upstream designs

The design considers Omagotchi (https://github.com/SLcode777/omagotchi) and OmaPets (https://github.com/yesmeck/OmaPets), but no code or assets are copied from either. Before adapting either repository, pin the exact commit and review code/asset licenses and attribution individually. A reference in the design does not confer redistribution rights.

Qt, QML runtimes and Omarchy/Quickshell components are external runtime dependencies, not redistributed by this repository; distribution packaging must review their applicable terms.
