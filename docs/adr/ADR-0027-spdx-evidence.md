# ADR-0027 SPDX 证据（渲染/字体依赖准入的许可证采证）

> **性质**：本文件是 **ADR-0027 §3 D3 的证据**，由 `cargo metadata` 在**仓库外**的探针包上采集（探针放在仓库内会让 cargo 把依赖树写进根 `Cargo.lock`，见 runbook 第 4 条铁律）。**它不是设计权威**：判定规则在 ADR-0015，准入决策在 ADR-0027。
> **采集日期**：2026-09-23
> **可复现命令**（探针在仓库外、自带空 `[workspace]`）：

```powershell
cargo metadata --format-version 1 --manifest-path <probe>/Cargo.toml > metadata.json
# 逐包取 name / version / license / license_file / source，按 ADR-0015 LB-01..LB-18 判 A/R/D
```

> **黑名单（GPL / AGPL / SSPL）命中：0**。唯一的弱 copyleft 是 `r-efi`，表达式 `MIT OR Apache-2.0 OR LGPL-2.1-or-later`，按 LB-05 择 MIT/Apache 分支。
> **非标准 SPDX 书写**：16 个包写作 `MIT/Apache-2.0`（crates.io 历史写法，SPDX 未定义 `/`）。它们**不是**「许可未知」，而是「已知许可用了非标准分隔符」；表中同时给出 `strict tier = U` 与 `normalized tier = A` 两列，**不隐藏归一化这一步**。

## 汇总（spdx-summary.json）

```
{
  "resolutionDate": "2026-09-23",
  "totalPackages": 270,
  "thirdPartyPackages": 269,
  "runtimeInBoundary": 236,
  "buildOrProcMacroOutOfBoundary": 33,
  "tierCounts": {
    "A": 254,
    "R": 0,
    "D": 0,
    "U": 16
  },
  "famCounts": {
    "Apache-2.0": 10,
    "MIT OR Apache-2.0": 129,
    "MIT": 62,
    "BSD-2-Clause": 1,
    "Apache-2.0 OR MIT": 17,
    "MIT/Apache-2.0": 12,
    "Zlib OR Apache-2.0 OR MIT": 9,
    "MIT OR Apache-2.0 OR Zlib": 4,
    "Apache-2.0 AND MIT": 1,
    "Zlib": 2,
    "ISC": 1,
    "Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT": 6,
    "Unlicense OR MIT": 3,
    "BSD-3-Clause OR MIT OR Apache-2.0": 2,
    "MIT OR Apache-2.0 OR LGPL-2.1-or-later": 2,
    "Apache-2.0/MIT": 1,
    "Unlicense/MIT": 2,
    "(missing)": 1,
    "BSD-3-Clause": 2,
    "(MIT OR Apache-2.0) AND Unicode-3.0": 1,
    "BSD-2-Clause OR Apache-2.0 OR MIT": 2
  },
  "weakCopyleft": [
    "r-efi@5.3.0 [MIT OR Apache-2.0 OR LGPL-2.1-or-later] LB-01+LB-05",
    "r-efi@6.0.0 [MIT OR Apache-2.0 OR LGPL-2.1-or-later] LB-03"
  ],
  "blacklistHits": [],
  "unknownLicenses": [
    "bitflags@1.3.2 [MIT/Apache-2.0] license_file=(none)",
    "downcast-rs@1.2.1 [MIT/Apache-2.0] license_file=(none)",
    "foreign-types@0.5.0 [MIT/Apache-2.0] license_file=(none)",
    "foreign-types-macros@0.2.4 [MIT/Apache-2.0] license_file=(none)",
    "foreign-types-shared@0.3.1 [MIT/Apache-2.0] license_file=(none)",
    "khronos-egl@6.0.0 [MIT/Apache-2.0] license_file=(none)",
    "plain@0.2.3 [MIT/Apache-2.0] license_file=(none)",
    "rustc-hash@1.1.0 [Apache-2.0/MIT] license_file=(none)",
    "same-file@1.0.6 [Unlicense/MIT] license_file=(none)",
    "scoped-tls@1.0.1 [MIT/Apache-2.0] license_file=(none)",
    "unicode-bidi-mirroring@0.4.0 [MIT/Apache-2.0] license_file=(none)",
    "unicode-ccc@0.4.0 [MIT/Apache-2.0] license_file=(none)",
    "unicode-properties@0.1.4 [MIT/Apache-2.0] license_file=(none)",
    "version_check@0.9.5 [MIT/Apache-2.0] license_file=(none)",
    "walkdir@2.5.0 [Unlicense/MIT] license_file=(none)"
  ],
  "approvalTier": [],
  "licenseFilePresent": [],
  "platforms": {
    "unfiltered": {
      "packages": 270,
      "runtimeReachable": 253
    },
    "win": {
      "packages": 113,
      "runtimeReachable": 106
    },
    "linux": {
      "packages": 142,
      "runtimeReachable": 134
    },
    "darwin": {
      "packages": 122,
      "runtimeReachable": 118
    },
    "winOnly": [
      "glutin_wgl_sys@0.6.1",
      "range-alloc@0.1.5",
      "unicode-segmentation@1.13.3",
      "winapi-util@0.1.11",
      "windows-collections@0.3.2",
      "windows-core@0.62.2",
      "windows-future@0.3.2",
      "windows-implement@0.60.2",
      "windows-interface@0.59.3",
      "windows-link@0.2.1",
      "windows-numerics@0.3.1",
      "windows-result@0.4.1",
      "windows-strings@0.5.1",
      "windows-sys@0.52.0",
      "windows-sys@0.61.2",
      "windows-targets@0.52.6",
      "windows-threading@0.2.1",
      "windows@0.62.2",
      "windows_x86_64_msvc@0.52.6"
    ],
    "linuxOnly": [
      "ab_glyph@0.2.32",
      "ab_glyph_rasterizer@0.1.10",
      "ahash@0.8.12",
      "arrayref@0.3.9",
      "as-raw-xcb-connection@1.0.1",
      "calloop-wayland-source@0.3.0",
      "calloop@0.13.0",
      "downcast-rs@1.2.1",
      "errno@0.3.14",
      "fontconfig-parser@0.5.8",
      "gethostname@1.1.0",
      "getrandom@0.3.4",
      "linux-raw-sys@0.12.1",
      "linux-raw-sys@0.4.15",
      "memchr@2.8.3",
      "owned_ttf_parser@0.25.1",
      "percent-encoding@2.3.2",
      "polling@3.11.0",
      "quick-xml@0.41.0",
      "roxmltree@0.20.0",
      "rustix@0.38.44",
      "rustix@1.1.5",
      "scoped-tls@1.0.1",
      "sctk-adwaita@0.10.1",
      "slab@0.4.12",
      "smithay-client-toolkit@0.19.2",
      "strict-num@0.1.1",
      "thiserror-impl@1.0.69",
      "thiserror@1.0.69",
      "tiny-skia-path@0.11.4",
      "tiny-skia@0.11.4",
      "wayland-backend@0.3.17",
      "wayland-client@0.31.15",
      "wayland-csd-frame@0.3.0",
      "wayland-cursor@0.31.14",
      "wayland-protocols-plasma@0.3.12",
      "wayland-protocols-wlr@0.3.12",
      "wayland-protocols@0.32.13",
      "wayland-scanner@0.31.11",
      "x11-dl@2.21.0",
      "x11rb-protocol@0.13.2",
      "x11rb@0.13.2",
      "xcursor@0.3.11",
      "xkbcommon-dl@0.4.2",
      "xkeysym@0.2.1"
    ],
    "darwinOnly": [
      "bitflags@1.3.2",
      "block2@0.5.1",
      "block2@0.6.2",
      "core-foundation-sys@0.8.7",
      "core-foundation@0.9.4",
      "core-graphics-types@0.1.3",
      "core-graphics@0.23.2",
      "dispatch2@0.3.1",
      "dispatch@0.2.0",
      "foreign-types-macros@0.2.4",
      "foreign-types-shared@0.3.1",
      "foreign-types@0.5.0",
      "objc-sys@0.3.5",
      "objc2-app-kit@0.2.2",
      "objc2-core-data@0.2.2",
      "objc2-core-foundation@0.3.2",
      "objc2-core-graphics@0.3.2",
      "objc2-core-image@0.2.2",
      "objc2-encode@4.1.0",
      "objc2-foundation@0.2.2",
      "objc2-foundation@0.3.2",
      "objc2-io-surface@0.3.2",
      "objc2-metal@0.2.2",
      "objc2-metal@0.3.2",
      "objc2-quartz-core@0.2.2",
      "objc2-quartz-core@0.3.2",
      "objc2@0.5.2",
      "objc2@0.6.4",
      "raw-window-metal@1.1.0",
      "wgpu-core-deps-apple@30.0.1"
    ],
    "inAllThree": [
      "allocator-api2@0.2.21",
      "arrayvec@0.7.8",
      "ash@0.38.0+1.3.281",
      "bit-set@0.10.0",
      "bit-vec@0.9.1",
      "bitflags@2.13.2",
      "bytemuck@1.25.2",
      "bytemuck_derive@1.12.1",
      "cfg-if@1.0.5",
      "codespan-reporting@0.13.1",
      "core_maths@0.1.1",
      "cursor-icon@1.2.0",
      "document-features@0.2.12",
      "dpi@0.1.2",
      "equivalent@1.0.2",
      "foldhash@0.2.0",
      "font-types@0.12.5",
      "fontdb@0.24.0",
      "glow@0.17.0",
      "gpu-allocator@0.28.0",
      "half@2.7.1",
      "hashbrown@0.16.1",
      "hashbrown@0.17.1",
      "indexmap@2.14.2",
      "khronos-egl@6.0.0",
      "libc@0.2.189",
      "libloading@0.8.9",
      "libm@0.2.16",
      "litrs@1.0.0",
      "lock_api@0.4.14",
      "log@0.4.34",
      "memmap2@0.9.11",
      "naga-types@30.0.1",
      "naga@30.0.1",
      "num-traits@0.2.19",
      "once_cell@1.21.4",
      "ordered-float@5.5.0",
      "parking_lot@0.12.5",
      "parking_lot_core@0.9.12",
      "pin-project-lite@0.2.17",
      "presser@0.3.1",
      "proc-macro2@1.0.107",
      "profiling@1.0.18",
      "quote@1.0.47",
      "raw-window-handle@0.6.2",
      "read-fonts@0.41.0",
      "renderdoc-sys@1.1.0",
      "rustc-hash@1.1.0",
      "rustybuzz@0.20.1",
      "scopeguard@1.2.0",
      "serde@1.0.229",
      "serde_core@1.0.229",
      "serde_derive@1.0.229",
      "skrifa@0.44.0",
      "slotmap@1.1.1",
      "smallvec@1.16.1",
      "smol_str@0.2.2",
      "spirv@0.4.0+sdk-1.4.341.0",
      "static_assertions@1.1.0",
      "swash@0.2.10",
      "syn@2.0.119",
      "syn@3.0.6",
      "termai-spdx-probe@0.0.0",
      "termcolor@1.4.1",
      "thiserror-impl@2.0.20",
      "thiserror@2.0.20",
      "tinyvec@1.13.3",
      "tracing-core@0.1.36",
      "tracing@0.1.44",
      "ttf-parser@0.25.1",
      "unicode-bidi-mirroring@0.4.0",
      "unicode-ccc@0.4.0",
      "unicode-ident@1.0.26",
      "unicode-properties@0.1.4",
      "unicode-script@0.5.8",
      "unicode-width@0.2.2",
      "wgpu-core@30.0.1",
      "wgpu-hal@30.0.1",
      "wgpu-naga-bridge@30.0.1",
      "wgpu-types@30.0.1",
      "wgpu@30.0.1",
      "winit@0.30.13",
      "yazi@0.2.1",
      "zeno@0.3.3",
      "zerocopy-derive@0.8.57",
      "zerocopy@0.8.57"
    ]
  },
  "directDeps": [
    "wgpu 30.0.1 [MIT OR Apache-2.0] A LB-01+LB-05",
    "winit 0.30.13 [Apache-2.0] A LB-01+LB-05",
    "rustybuzz 0.20.1 [MIT] A LB-01+LB-05",
    "swash 0.2.10 [Apache-2.0 OR MIT] A LB-01+LB-05",
    "fontdb 0.24.0 [MIT] A LB-01+LB-05"
  ]
}
```

## 全量逐包表（runtime 与 build-only 分列；A/R/D/U 为 ADR-0015 判定档）

| # | crate | version | license (cargo metadata) | normalized SPDX | source | scope | LB rule | in boundary | strict tier | normalized tier | verdict | license_file | repository |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | ab_glyph | 0.2.32 | Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/alexheretic/ab-glyph](https://github.com/alexheretic/ab-glyph) |
| 2 | ab_glyph_rasterizer | 0.1.10 | Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/alexheretic/ab-glyph](https://github.com/alexheretic/ab-glyph) |
| 3 | ahash | 0.8.12 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/tkaitchuck/ahash](https://github.com/tkaitchuck/ahash) |
| 4 | allocator-api2 | 0.2.21 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/zakarumych/allocator-api2](https://github.com/zakarumych/allocator-api2) |
| 5 | android-activity | 0.6.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/rust-mobile/android-activity](https://github.com/rust-mobile/android-activity) |
| 6 | android-properties | 0.2.2 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/miklelappo/android-properties](https://github.com/miklelappo/android-properties) |
| 7 | android_system_properties | 0.1.6 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/nical/android_system_properties](https://github.com/nical/android_system_properties) |
| 8 | arrayref | 0.3.9 | BSD-2-Clause | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/droundy/arrayref](https://github.com/droundy/arrayref) |
| 9 | arrayvec | 0.7.8 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/bluss/arrayvec](https://github.com/bluss/arrayvec) |
| 10 | as-raw-xcb-connection | 1.0.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/psychon/as-raw-xcb-connection](https://github.com/psychon/as-raw-xcb-connection) |
| 11 | ash | 0.38.0+1.3.281 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/ash-rs/ash](https://github.com/ash-rs/ash) |
| 12 | atomic-waker | 1.1.2 | Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/smol-rs/atomic-waker](https://github.com/smol-rs/atomic-waker) |
| 13 | autocfg | 1.5.1 | Apache-2.0 OR MIT | - | crates.io | build-only | LB-03 | no | A | A | A | - | [github.com/cuviper/autocfg](https://github.com/cuviper/autocfg) |
| 14 | bit-set | 0.10.0 | Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/contain-rs/bit-set](https://github.com/contain-rs/bit-set) |
| 15 | bit-vec | 0.9.1 | Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/contain-rs/bit-vec](https://github.com/contain-rs/bit-vec) |
| 16 | bitflags | 1.3.2 | MIT/Apache-2.0 | MIT OR Apache-2.0 | crates.io | runtime (normal) | LB-01 + LB-05 | yes | U | A | A | - | [github.com/bitflags/bitflags](https://github.com/bitflags/bitflags) |
| 17 | bitflags | 2.13.2 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/bitflags/bitflags](https://github.com/bitflags/bitflags) |
| 18 | block2 | 0.5.1 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 19 | block2 | 0.6.2 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 20 | bumpalo | 3.20.3 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/fitzgen/bumpalo](https://github.com/fitzgen/bumpalo) |
| 21 | bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/Lokathor/bytemuck](https://github.com/Lokathor/bytemuck) |
| 22 | bytemuck_derive | 1.12.1 | Zlib OR Apache-2.0 OR MIT | - | crates.io | build-time proc-macro | LB-04 | no | A | A | A | - | [github.com/Lokathor/bytemuck](https://github.com/Lokathor/bytemuck) |
| 23 | bytes | 1.12.1 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/tokio-rs/bytes](https://github.com/tokio-rs/bytes) |
| 24 | calloop | 0.13.0 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/Smithay/calloop](https://github.com/Smithay/calloop) |
| 25 | calloop-wayland-source | 0.3.0 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/smithay/calloop-wayland-source](https://github.com/smithay/calloop-wayland-source) |
| 26 | cc | 1.4.7 | MIT OR Apache-2.0 | - | crates.io | build-only | LB-03 | no | A | A | A | - | [github.com/rust-lang/cc-rs](https://github.com/rust-lang/cc-rs) |
| 27 | cfg-if | 1.0.5 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/rust-lang/cfg-if](https://github.com/rust-lang/cfg-if) |
| 28 | cfg_aliases | 0.2.2 | MIT | - | crates.io | build-only | LB-03 | no | A | A | A | - | [github.com/katharostech/cfg_aliases](https://github.com/katharostech/cfg_aliases) |
| 29 | codespan-reporting | 0.13.1 | Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/brendanzab/codespan](https://github.com/brendanzab/codespan) |
| 30 | combine | 4.6.8 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/Marwes/combine](https://github.com/Marwes/combine) |
| 31 | concurrent-queue | 2.5.0 | Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/smol-rs/concurrent-queue](https://github.com/smol-rs/concurrent-queue) |
| 32 | core-foundation | 0.9.4 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/servo/core-foundation-rs](https://github.com/servo/core-foundation-rs) |
| 33 | core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/servo/core-foundation-rs](https://github.com/servo/core-foundation-rs) |
| 34 | core-graphics | 0.23.2 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/servo/core-foundation-rs](https://github.com/servo/core-foundation-rs) |
| 35 | core-graphics-types | 0.1.3 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/servo/core-foundation-rs](https://github.com/servo/core-foundation-rs) |
| 36 | core_maths | 0.1.1 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/robertbastian/core_maths](https://github.com/robertbastian/core_maths) |
| 37 | crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/crossbeam-rs/crossbeam](https://github.com/crossbeam-rs/crossbeam) |
| 38 | crunchy | 0.2.4 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/eira-fransham/crunchy](https://github.com/eira-fransham/crunchy) |
| 39 | cursor-icon | 1.2.0 | MIT OR Apache-2.0 OR Zlib | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/rust-windowing/cursor-icon](https://github.com/rust-windowing/cursor-icon) |
| 40 | dispatch | 0.2.0 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [http://github.com/SSheldon/rust-dispatch](http://github.com/SSheldon/rust-dispatch) |
| 41 | dispatch2 | 0.3.1 | Zlib OR Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 42 | dlib | 0.5.3 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/elinorbgr/dlib](https://github.com/elinorbgr/dlib) |
| 43 | document-features | 0.2.12 | MIT OR Apache-2.0 | - | crates.io | build-time proc-macro | LB-04 | no | A | A | A | - | [github.com/slint-ui/document-features](https://github.com/slint-ui/document-features) |
| 44 | downcast-rs | 1.2.1 | MIT/Apache-2.0 | MIT OR Apache-2.0 | crates.io | runtime (normal) | LB-01 + LB-05 | yes | U | A | A | - | [github.com/marcianx/downcast-rs](https://github.com/marcianx/downcast-rs) |
| 45 | dpi | 0.1.2 | Apache-2.0 AND MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/rust-windowing/winit](https://github.com/rust-windowing/winit) |
| 46 | equivalent | 1.0.2 | Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/indexmap-rs/equivalent](https://github.com/indexmap-rs/equivalent) |
| 47 | errno | 0.3.14 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/lambda-fairy/rust-errno](https://github.com/lambda-fairy/rust-errno) |
| 48 | find-msvc-tools | 0.1.13 | MIT OR Apache-2.0 | - | crates.io | build-only | LB-03 | no | A | A | A | - | [github.com/rust-lang/cc-rs](https://github.com/rust-lang/cc-rs) |
| 49 | foldhash | 0.2.0 | Zlib | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/orlp/foldhash](https://github.com/orlp/foldhash) |
| 50 | font-types | 0.12.5 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/googlefonts/fontations](https://github.com/googlefonts/fontations) |
| 51 | fontconfig-parser | 0.5.8 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/Riey/fontconfig-parser](https://github.com/Riey/fontconfig-parser) |
| 52 | fontdb | 0.24.0 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/RazrFalcon/fontdb](https://github.com/RazrFalcon/fontdb) |
| 53 | foreign-types | 0.5.0 | MIT/Apache-2.0 | MIT OR Apache-2.0 | crates.io | runtime (normal) | LB-01 + LB-05 | yes | U | A | A | - | [github.com/sfackler/foreign-types](https://github.com/sfackler/foreign-types) |
| 54 | foreign-types-macros | 0.2.4 | MIT/Apache-2.0 | MIT OR Apache-2.0 | crates.io | build-time proc-macro | LB-04 | no | U | A | A | - | [github.com/sfackler/foreign-types](https://github.com/sfackler/foreign-types) |
| 55 | foreign-types-shared | 0.3.1 | MIT/Apache-2.0 | MIT OR Apache-2.0 | crates.io | runtime (normal) | LB-01 + LB-05 | yes | U | A | A | - | [github.com/sfackler/foreign-types](https://github.com/sfackler/foreign-types) |
| 56 | futures-core | 0.3.34 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/rust-lang/futures-rs](https://github.com/rust-lang/futures-rs) |
| 57 | futures-task | 0.3.34 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/rust-lang/futures-rs](https://github.com/rust-lang/futures-rs) |
| 58 | futures-util | 0.3.34 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/rust-lang/futures-rs](https://github.com/rust-lang/futures-rs) |
| 59 | gethostname | 1.1.0 | Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [codeberg.org/swsnr/gethostname.rs.git](https://codeberg.org/swsnr/gethostname.rs.git) |
| 60 | getrandom | 0.3.4 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/rust-random/getrandom](https://github.com/rust-random/getrandom) |
| 61 | getrandom | 0.4.3 | MIT OR Apache-2.0 | - | crates.io | build-only | LB-03 | no | A | A | A | - | [github.com/rust-random/getrandom](https://github.com/rust-random/getrandom) |
| 62 | gl_generator | 0.14.0 | Apache-2.0 | - | crates.io | build-only | LB-03 | no | A | A | A | - | [github.com/brendanzab/gl-rs/](https://github.com/brendanzab/gl-rs/) |
| 63 | glow | 0.17.0 | MIT OR Apache-2.0 OR Zlib | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/grovesNL/glow](https://github.com/grovesNL/glow) |
| 64 | glutin_wgl_sys | 0.6.1 | Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/rust-windowing/glutin](https://github.com/rust-windowing/glutin) |
| 65 | gpu-allocator | 0.28.0 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/Traverse-Research/gpu-allocator](https://github.com/Traverse-Research/gpu-allocator) |
| 66 | half | 2.7.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/VoidStarKat/half-rs](https://github.com/VoidStarKat/half-rs) |
| 67 | hashbrown | 0.16.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/rust-lang/hashbrown](https://github.com/rust-lang/hashbrown) |
| 68 | hashbrown | 0.17.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/rust-lang/hashbrown](https://github.com/rust-lang/hashbrown) |
| 69 | hermit-abi | 0.5.3 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/hermit-os/hermit-rs](https://github.com/hermit-os/hermit-rs) |
| 70 | indexmap | 2.14.2 | Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/indexmap-rs/indexmap](https://github.com/indexmap-rs/indexmap) |
| 71 | jni | 0.22.4 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/jni-rs/jni-rs](https://github.com/jni-rs/jni-rs) |
| 72 | jni-macros | 0.22.4 | MIT OR Apache-2.0 | - | crates.io | build-time proc-macro | LB-04 | no | A | A | A | - | [github.com/jni-rs/jni-rs](https://github.com/jni-rs/jni-rs) |
| 73 | jni-sys | 0.3.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/jni-rs/jni-sys](https://github.com/jni-rs/jni-sys) |
| 74 | jni-sys | 0.4.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/jni-rs/jni-sys](https://github.com/jni-rs/jni-sys) |
| 75 | jni-sys-macros | 0.4.1 | MIT OR Apache-2.0 | - | crates.io | build-time proc-macro | LB-04 | no | A | A | A | - | [github.com/jni-rs/jni-sys](https://github.com/jni-rs/jni-sys) |
| 76 | jobserver | 0.1.35 | MIT OR Apache-2.0 | - | crates.io | build-only | LB-03 | no | A | A | A | - | [github.com/rust-lang/jobserver-rs](https://github.com/rust-lang/jobserver-rs) |
| 77 | js-sys | 0.3.105 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/js-sys](https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/js-sys) |
| 78 | khronos-egl | 6.0.0 | MIT/Apache-2.0 | MIT OR Apache-2.0 | crates.io | runtime (normal) | LB-01 + LB-05 | yes | U | A | A | - | [github.com/timothee-haudebourg/khronos-egl](https://github.com/timothee-haudebourg/khronos-egl) |
| 79 | khronos_api | 3.1.0 | Apache-2.0 | - | crates.io | build-only | LB-03 | no | A | A | A | - | [github.com/brendanzab/gl-rs/](https://github.com/brendanzab/gl-rs/) |
| 80 | libc | 0.2.189 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/rust-lang/libc](https://github.com/rust-lang/libc) |
| 81 | libloading | 0.8.9 | ISC | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/nagisa/rust_libloading/](https://github.com/nagisa/rust_libloading/) |
| 82 | libm | 0.2.16 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/rust-lang/compiler-builtins](https://github.com/rust-lang/compiler-builtins) |
| 83 | libredox | 0.1.25 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [gitlab.redox-os.org/redox-os/libredox.git](https://gitlab.redox-os.org/redox-os/libredox.git) |
| 84 | linux-raw-sys | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/sunfishcode/linux-raw-sys](https://github.com/sunfishcode/linux-raw-sys) |
| 85 | linux-raw-sys | 0.4.15 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/sunfishcode/linux-raw-sys](https://github.com/sunfishcode/linux-raw-sys) |
| 86 | litrs | 1.0.0 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/LukasKalbertodt/litrs](https://github.com/LukasKalbertodt/litrs) |
| 87 | lock_api | 0.4.14 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/Amanieu/parking_lot](https://github.com/Amanieu/parking_lot) |
| 88 | log | 0.4.34 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/rust-lang/log](https://github.com/rust-lang/log) |
| 89 | memchr | 2.8.3 | Unlicense OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/BurntSushi/memchr](https://github.com/BurntSushi/memchr) |
| 90 | memmap2 | 0.9.11 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/RazrFalcon/memmap2-rs](https://github.com/RazrFalcon/memmap2-rs) |
| 91 | naga | 30.0.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/gfx-rs/wgpu](https://github.com/gfx-rs/wgpu) |
| 92 | naga-types | 30.0.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/gfx-rs/wgpu](https://github.com/gfx-rs/wgpu) |
| 93 | ndk | 0.9.0 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/rust-mobile/ndk](https://github.com/rust-mobile/ndk) |
| 94 | ndk-context | 0.1.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/rust-windowing/android-ndk-rs](https://github.com/rust-windowing/android-ndk-rs) |
| 95 | ndk-sys | 0.6.0+11769913 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/rust-mobile/ndk](https://github.com/rust-mobile/ndk) |
| 96 | num-traits | 0.2.19 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/rust-num/num-traits](https://github.com/rust-num/num-traits) |
| 97 | num_enum | 0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/illicitonion/num_enum](https://github.com/illicitonion/num_enum) |
| 98 | num_enum_derive | 0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 | - | crates.io | build-time proc-macro | LB-04 | no | A | A | A | - | [github.com/illicitonion/num_enum](https://github.com/illicitonion/num_enum) |
| 99 | objc-sys | 0.3.5 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 100 | objc2 | 0.5.2 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 101 | objc2 | 0.6.4 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 102 | objc2-app-kit | 0.2.2 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 103 | objc2-cloud-kit | 0.2.2 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 104 | objc2-contacts | 0.2.2 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 105 | objc2-core-data | 0.2.2 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 106 | objc2-core-foundation | 0.3.2 | Zlib OR Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 107 | objc2-core-graphics | 0.3.2 | Zlib OR Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 108 | objc2-core-image | 0.2.2 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 109 | objc2-core-location | 0.2.2 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 110 | objc2-encode | 4.1.0 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 111 | objc2-foundation | 0.2.2 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 112 | objc2-foundation | 0.3.2 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 113 | objc2-io-surface | 0.3.2 | Zlib OR Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 114 | objc2-link-presentation | 0.2.2 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 115 | objc2-metal | 0.2.2 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 116 | objc2-metal | 0.3.2 | Zlib OR Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 117 | objc2-quartz-core | 0.2.2 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 118 | objc2-quartz-core | 0.3.2 | Zlib OR Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 119 | objc2-symbols | 0.2.2 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 120 | objc2-ui-kit | 0.2.2 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 121 | objc2-uniform-type-identifiers | 0.2.2 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 122 | objc2-user-notifications | 0.2.2 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| 123 | once_cell | 1.21.4 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/matklad/once_cell](https://github.com/matklad/once_cell) |
| 124 | orbclient | 0.3.55 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [gitlab.redox-os.org/redox-os/orbclient](https://gitlab.redox-os.org/redox-os/orbclient) |
| 125 | ordered-float | 5.5.0 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/reem/rust-ordered-float](https://github.com/reem/rust-ordered-float) |
| 126 | owned_ttf_parser | 0.25.1 | Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/alexheretic/owned-ttf-parser](https://github.com/alexheretic/owned-ttf-parser) |
| 127 | parking_lot | 0.12.5 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/Amanieu/parking_lot](https://github.com/Amanieu/parking_lot) |
| 128 | parking_lot_core | 0.9.12 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/Amanieu/parking_lot](https://github.com/Amanieu/parking_lot) |
| 129 | percent-encoding | 2.3.2 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/servo/rust-url/](https://github.com/servo/rust-url/) |
| 130 | pin-project | 1.1.13 | Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/taiki-e/pin-project](https://github.com/taiki-e/pin-project) |
| 131 | pin-project-internal | 1.1.13 | Apache-2.0 OR MIT | - | crates.io | build-time proc-macro | LB-04 | no | A | A | A | - | [github.com/taiki-e/pin-project](https://github.com/taiki-e/pin-project) |
| 132 | pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/taiki-e/pin-project-lite](https://github.com/taiki-e/pin-project-lite) |
| 133 | pkg-config | 0.3.34 | MIT OR Apache-2.0 | - | crates.io | build-only | LB-03 | no | A | A | A | - | [github.com/rust-lang/pkg-config-rs](https://github.com/rust-lang/pkg-config-rs) |
| 134 | plain | 0.2.3 | MIT/Apache-2.0 | MIT OR Apache-2.0 | crates.io | runtime (normal) | LB-01 + LB-05 | yes | U | A | A | - | [github.com/randomites/plain](https://github.com/randomites/plain) |
| 135 | polling | 3.11.0 | Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/smol-rs/polling](https://github.com/smol-rs/polling) |
| 136 | portable-atomic | 1.15.0 | Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/taiki-e/portable-atomic](https://github.com/taiki-e/portable-atomic) |
| 137 | portable-atomic-util | 0.2.8 | Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/taiki-e/portable-atomic-util](https://github.com/taiki-e/portable-atomic-util) |
| 138 | presser | 0.3.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/EmbarkStudios/presser](https://github.com/EmbarkStudios/presser) |
| 139 | proc-macro-crate | 3.5.0 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/bkchr/proc-macro-crate](https://github.com/bkchr/proc-macro-crate) |
| 140 | proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/dtolnay/proc-macro2](https://github.com/dtolnay/proc-macro2) |
| 141 | profiling | 1.0.18 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/aclysma/profiling](https://github.com/aclysma/profiling) |
| 142 | quick-xml | 0.41.0 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/tafia/quick-xml](https://github.com/tafia/quick-xml) |
| 143 | quote | 1.0.47 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/dtolnay/quote](https://github.com/dtolnay/quote) |
| 144 | r-efi | 5.3.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/r-efi/r-efi](https://github.com/r-efi/r-efi) |
| 145 | r-efi | 6.0.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | - | crates.io | build-only | LB-03 | no | A | A | A | - | [github.com/r-efi/r-efi](https://github.com/r-efi/r-efi) |
| 146 | range-alloc | 0.1.5 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/gfx-rs/range-alloc](https://github.com/gfx-rs/range-alloc) |
| 147 | raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/rust-windowing/raw-window-handle](https://github.com/rust-windowing/raw-window-handle) |
| 148 | raw-window-metal | 1.1.0 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/rust-windowing/raw-window-metal](https://github.com/rust-windowing/raw-window-metal) |
| 149 | read-fonts | 0.41.0 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/googlefonts/fontations](https://github.com/googlefonts/fontations) |
| 150 | redox_syscall | 0.4.1 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [gitlab.redox-os.org/redox-os/syscall](https://gitlab.redox-os.org/redox-os/syscall) |
| 151 | redox_syscall | 0.5.18 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [gitlab.redox-os.org/redox-os/syscall](https://gitlab.redox-os.org/redox-os/syscall) |
| 152 | redox_syscall | 0.9.4 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [gitlab.redox-os.org/redox-os/kernel](https://gitlab.redox-os.org/redox-os/kernel) |
| 153 | renderdoc-sys | 1.1.0 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/ebkalderon/renderdoc-rs](https://github.com/ebkalderon/renderdoc-rs) |
| 154 | roxmltree | 0.20.0 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/RazrFalcon/roxmltree](https://github.com/RazrFalcon/roxmltree) |
| 155 | rustc-hash | 1.1.0 | Apache-2.0/MIT | Apache-2.0 OR MIT | crates.io | runtime (normal) | LB-01 + LB-05 | yes | U | A | A | - | [github.com/rust-lang-nursery/rustc-hash](https://github.com/rust-lang-nursery/rustc-hash) |
| 156 | rustc_version | 0.4.1 | MIT OR Apache-2.0 | - | crates.io | build-only | LB-03 | no | A | A | A | - | [github.com/djc/rustc-version-rs](https://github.com/djc/rustc-version-rs) |
| 157 | rustix | 0.38.44 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/bytecodealliance/rustix](https://github.com/bytecodealliance/rustix) |
| 158 | rustix | 1.1.5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/bytecodealliance/rustix](https://github.com/bytecodealliance/rustix) |
| 159 | rustversion | 1.0.23 | MIT OR Apache-2.0 | - | crates.io | build-time proc-macro | LB-04 | no | A | A | A | - | [github.com/dtolnay/rustversion](https://github.com/dtolnay/rustversion) |
| 160 | rustybuzz | 0.20.1 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/harfbuzz/rustybuzz](https://github.com/harfbuzz/rustybuzz) |
| 161 | same-file | 1.0.6 | Unlicense/MIT | Unlicense OR MIT | crates.io | build-only | LB-03 | no | U | A | A | - | [github.com/BurntSushi/same-file](https://github.com/BurntSushi/same-file) |
| 162 | scoped-tls | 1.0.1 | MIT/Apache-2.0 | MIT OR Apache-2.0 | crates.io | runtime (normal) | LB-01 + LB-05 | yes | U | A | A | - | [github.com/alexcrichton/scoped-tls](https://github.com/alexcrichton/scoped-tls) |
| 163 | scopeguard | 1.2.0 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/bluss/scopeguard](https://github.com/bluss/scopeguard) |
| 164 | sctk-adwaita | 0.10.1 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/PolyMeilex/sctk-adwaita](https://github.com/PolyMeilex/sctk-adwaita) |
| 165 | semver | 1.0.28 | MIT OR Apache-2.0 | - | crates.io | build-only | LB-03 | no | A | A | A | - | [github.com/dtolnay/semver](https://github.com/dtolnay/semver) |
| 166 | serde | 1.0.229 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/serde-rs/serde](https://github.com/serde-rs/serde) |
| 167 | serde_core | 1.0.229 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/serde-rs/serde](https://github.com/serde-rs/serde) |
| 168 | serde_derive | 1.0.229 | MIT OR Apache-2.0 | - | crates.io | build-time proc-macro | LB-04 | no | A | A | A | - | [github.com/serde-rs/serde](https://github.com/serde-rs/serde) |
| 169 | shlex | 2.0.1 | MIT OR Apache-2.0 | - | crates.io | build-only | LB-03 | no | A | A | A | - | [github.com/comex/rust-shlex](https://github.com/comex/rust-shlex) |
| 170 | simd_cesu8 | 1.2.0 | Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/seancroach/simd_cesu8](https://github.com/seancroach/simd_cesu8) |
| 171 | simdutf8 | 0.1.5 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/rusticstuff/simdutf8](https://github.com/rusticstuff/simdutf8) |
| 172 | skrifa | 0.44.0 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/googlefonts/fontations](https://github.com/googlefonts/fontations) |
| 173 | slab | 0.4.12 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/tokio-rs/slab](https://github.com/tokio-rs/slab) |
| 174 | slotmap | 1.1.1 | Zlib | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/orlp/slotmap](https://github.com/orlp/slotmap) |
| 175 | smallvec | 1.16.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/servo/rust-smallvec](https://github.com/servo/rust-smallvec) |
| 176 | smithay-client-toolkit | 0.19.2 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/smithay/client-toolkit](https://github.com/smithay/client-toolkit) |
| 177 | smol_str | 0.2.2 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/rust-analyzer/smol_str](https://github.com/rust-analyzer/smol_str) |
| 178 | spirv | 0.4.0+sdk-1.4.341.0 | Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/gfx-rs/rspirv](https://github.com/gfx-rs/rspirv) |
| 179 | static_assertions | 1.1.0 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/nvzqz/static-assertions-rs](https://github.com/nvzqz/static-assertions-rs) |
| 180 | strict-num | 0.1.1 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/RazrFalcon/strict-num](https://github.com/RazrFalcon/strict-num) |
| 181 | swash | 0.2.10 | Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/dfrg/swash](https://github.com/dfrg/swash) |
| 182 | syn | 2.0.119 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/dtolnay/syn](https://github.com/dtolnay/syn) |
| 183 | syn | 3.0.6 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/dtolnay/syn](https://github.com/dtolnay/syn) |
| 184 | termcolor | 1.4.1 | Unlicense OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/BurntSushi/termcolor](https://github.com/BurntSushi/termcolor) |
| 185 | thiserror | 1.0.69 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/dtolnay/thiserror](https://github.com/dtolnay/thiserror) |
| 186 | thiserror | 2.0.20 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/dtolnay/thiserror](https://github.com/dtolnay/thiserror) |
| 187 | thiserror-impl | 1.0.69 | MIT OR Apache-2.0 | - | crates.io | build-time proc-macro | LB-04 | no | A | A | A | - | [github.com/dtolnay/thiserror](https://github.com/dtolnay/thiserror) |
| 188 | thiserror-impl | 2.0.20 | MIT OR Apache-2.0 | - | crates.io | build-time proc-macro | LB-04 | no | A | A | A | - | [github.com/dtolnay/thiserror](https://github.com/dtolnay/thiserror) |
| 189 | tiny-skia | 0.11.4 | BSD-3-Clause | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/RazrFalcon/tiny-skia](https://github.com/RazrFalcon/tiny-skia) |
| 190 | tiny-skia-path | 0.11.4 | BSD-3-Clause | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/RazrFalcon/tiny-skia/tree/master/path](https://github.com/RazrFalcon/tiny-skia/tree/master/path) |
| 191 | tinyvec | 1.13.3 | Zlib OR Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/Lokathor/tinyvec](https://github.com/Lokathor/tinyvec) |
| 192 | toml_datetime | 1.1.1+spec-1.1.0 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/toml-rs/toml](https://github.com/toml-rs/toml) |
| 193 | toml_edit | 0.25.15+spec-1.1.0 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/toml-rs/toml](https://github.com/toml-rs/toml) |
| 194 | toml_parser | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/toml-rs/toml](https://github.com/toml-rs/toml) |
| 195 | tracing | 0.1.44 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/tokio-rs/tracing](https://github.com/tokio-rs/tracing) |
| 196 | tracing-core | 0.1.36 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/tokio-rs/tracing](https://github.com/tokio-rs/tracing) |
| 197 | ttf-parser | 0.25.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/harfbuzz/ttf-parser](https://github.com/harfbuzz/ttf-parser) |
| 198 | unicode-bidi-mirroring | 0.4.0 | MIT/Apache-2.0 | MIT OR Apache-2.0 | crates.io | runtime (normal) | LB-01 + LB-05 | yes | U | A | A | - | [github.com/RazrFalcon/unicode-bidi-mirroring](https://github.com/RazrFalcon/unicode-bidi-mirroring) |
| 199 | unicode-ccc | 0.4.0 | MIT/Apache-2.0 | MIT OR Apache-2.0 | crates.io | runtime (normal) | LB-01 + LB-05 | yes | U | A | A | - | [github.com/RazrFalcon/unicode-ccc](https://github.com/RazrFalcon/unicode-ccc) |
| 200 | unicode-ident | 1.0.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/dtolnay/unicode-ident](https://github.com/dtolnay/unicode-ident) |
| 201 | unicode-properties | 0.1.4 | MIT/Apache-2.0 | MIT OR Apache-2.0 | crates.io | runtime (normal) | LB-01 + LB-05 | yes | U | A | A | - | [github.com/unicode-rs/unicode-properties](https://github.com/unicode-rs/unicode-properties) |
| 202 | unicode-script | 0.5.8 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/unicode-rs/unicode-script](https://github.com/unicode-rs/unicode-script) |
| 203 | unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/unicode-rs/unicode-segmentation](https://github.com/unicode-rs/unicode-segmentation) |
| 204 | unicode-width | 0.2.2 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/unicode-rs/unicode-width](https://github.com/unicode-rs/unicode-width) |
| 205 | version_check | 0.9.5 | MIT/Apache-2.0 | MIT OR Apache-2.0 | crates.io | build-only | LB-03 | no | U | A | A | - | [github.com/SergioBenitez/version_check](https://github.com/SergioBenitez/version_check) |
| 206 | walkdir | 2.5.0 | Unlicense/MIT | Unlicense OR MIT | crates.io | build-only | LB-03 | no | U | A | A | - | [github.com/BurntSushi/walkdir](https://github.com/BurntSushi/walkdir) |
| 207 | wasip2 | 1.0.4+wasi-0.2.12 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/bytecodealliance/wasi-rs](https://github.com/bytecodealliance/wasi-rs) |
| 208 | wasm-bindgen | 0.2.128 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/wasm-bindgen/wasm-bindgen](https://github.com/wasm-bindgen/wasm-bindgen) |
| 209 | wasm-bindgen-futures | 0.4.78 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/futures](https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/futures) |
| 210 | wasm-bindgen-macro | 0.2.128 | MIT OR Apache-2.0 | - | crates.io | build-time proc-macro | LB-04 | no | A | A | A | - | [github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/macro](https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/macro) |
| 211 | wasm-bindgen-macro-support | 0.2.128 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/wasm-bindgen/wasm-bindgen/tree/main/crates/macro-support](https://github.com/wasm-bindgen/wasm-bindgen/tree/main/crates/macro-support) |
| 212 | wasm-bindgen-shared | 0.2.128 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/shared](https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/shared) |
| 213 | wayland-backend | 0.3.17 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/smithay/wayland-rs](https://github.com/smithay/wayland-rs) |
| 214 | wayland-client | 0.31.15 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/smithay/wayland-rs](https://github.com/smithay/wayland-rs) |
| 215 | wayland-csd-frame | 0.3.0 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/rust-windowing/wayland-csd-frame](https://github.com/rust-windowing/wayland-csd-frame) |
| 216 | wayland-cursor | 0.31.14 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/smithay/wayland-rs](https://github.com/smithay/wayland-rs) |
| 217 | wayland-protocols | 0.32.13 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/smithay/wayland-rs](https://github.com/smithay/wayland-rs) |
| 218 | wayland-protocols-plasma | 0.3.12 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/smithay/wayland-rs](https://github.com/smithay/wayland-rs) |
| 219 | wayland-protocols-wlr | 0.3.12 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/smithay/wayland-rs](https://github.com/smithay/wayland-rs) |
| 220 | wayland-scanner | 0.31.11 | MIT | - | crates.io | build-time proc-macro | LB-04 | no | A | A | A | - | [github.com/smithay/wayland-rs](https://github.com/smithay/wayland-rs) |
| 221 | wayland-sys | 0.31.11 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/smithay/wayland-rs](https://github.com/smithay/wayland-rs) |
| 222 | web-sys | 0.3.105 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/web-sys](https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/web-sys) |
| 223 | web-time | 1.1.0 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/daxpedda/web-time](https://github.com/daxpedda/web-time) |
| 224 | wgpu | 30.0.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/gfx-rs/wgpu](https://github.com/gfx-rs/wgpu) |
| 225 | wgpu-core | 30.0.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/gfx-rs/wgpu](https://github.com/gfx-rs/wgpu) |
| 226 | wgpu-core-deps-apple | 30.0.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/gfx-rs/wgpu](https://github.com/gfx-rs/wgpu) |
| 227 | wgpu-core-deps-emscripten | 30.0.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/gfx-rs/wgpu](https://github.com/gfx-rs/wgpu) |
| 228 | wgpu-core-deps-windows-linux-android | 30.0.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/gfx-rs/wgpu](https://github.com/gfx-rs/wgpu) |
| 229 | wgpu-hal | 30.0.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/gfx-rs/wgpu](https://github.com/gfx-rs/wgpu) |
| 230 | wgpu-naga-bridge | 30.0.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/gfx-rs/wgpu](https://github.com/gfx-rs/wgpu) |
| 231 | wgpu-types | 30.0.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/gfx-rs/wgpu](https://github.com/gfx-rs/wgpu) |
| 232 | winapi-util | 0.1.11 | Unlicense OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/BurntSushi/winapi-util](https://github.com/BurntSushi/winapi-util) |
| 233 | windows | 0.62.2 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| 234 | windows-collections | 0.3.2 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| 235 | windows-core | 0.62.2 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| 236 | windows-future | 0.3.2 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| 237 | windows-implement | 0.60.2 | MIT OR Apache-2.0 | - | crates.io | build-time proc-macro | LB-04 | no | A | A | A | - | [github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| 238 | windows-interface | 0.59.3 | MIT OR Apache-2.0 | - | crates.io | build-time proc-macro | LB-04 | no | A | A | A | - | [github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| 239 | windows-link | 0.2.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| 240 | windows-numerics | 0.3.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| 241 | windows-result | 0.4.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| 242 | windows-strings | 0.5.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| 243 | windows-sys | 0.52.0 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| 244 | windows-sys | 0.59.0 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| 245 | windows-sys | 0.61.2 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| 246 | windows-targets | 0.52.6 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| 247 | windows-threading | 0.2.1 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| 248 | windows_aarch64_gnullvm | 0.52.6 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| 249 | windows_aarch64_msvc | 0.52.6 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| 250 | windows_i686_gnu | 0.52.6 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| 251 | windows_i686_gnullvm | 0.52.6 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| 252 | windows_i686_msvc | 0.52.6 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| 253 | windows_x86_64_gnu | 0.52.6 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| 254 | windows_x86_64_gnullvm | 0.52.6 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| 255 | windows_x86_64_msvc | 0.52.6 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| 256 | winit | 0.30.13 | Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/rust-windowing/winit](https://github.com/rust-windowing/winit) |
| 257 | winnow | 1.0.4 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/winnow-rs/winnow](https://github.com/winnow-rs/winnow) |
| 258 | wit-bindgen | 0.57.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/bytecodealliance/wit-bindgen](https://github.com/bytecodealliance/wit-bindgen) |
| 259 | x11-dl | 2.21.0 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/AltF02/x11-rs.git](https://github.com/AltF02/x11-rs.git) |
| 260 | x11rb | 0.13.2 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/psychon/x11rb](https://github.com/psychon/x11rb) |
| 261 | x11rb-protocol | 0.13.2 | MIT OR Apache-2.0 | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/psychon/x11rb](https://github.com/psychon/x11rb) |
| 262 | xcursor | 0.3.11 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/esposm03/xcursor-rs](https://github.com/esposm03/xcursor-rs) |
| 263 | xkbcommon-dl | 0.4.2 | MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/rust-windowing/xkbcommon-dl](https://github.com/rust-windowing/xkbcommon-dl) |
| 264 | xkeysym | 0.2.1 | MIT OR Apache-2.0 OR Zlib | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/notgull/xkeysym](https://github.com/notgull/xkeysym) |
| 265 | xml-rs | 0.8.29 | MIT | - | crates.io | build-only | LB-03 | no | A | A | A | - | [github.com/kornelski/xml-rs](https://github.com/kornelski/xml-rs) |
| 266 | yazi | 0.2.1 | Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/dfrg/yazi](https://github.com/dfrg/yazi) |
| 267 | zeno | 0.3.3 | Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/dfrg/zeno](https://github.com/dfrg/zeno) |
| 268 | zerocopy | 0.8.57 | BSD-2-Clause OR Apache-2.0 OR MIT | - | crates.io | runtime (normal) | LB-01 + LB-05 | yes | A | A | A | - | [github.com/google/zerocopy](https://github.com/google/zerocopy) |
| 269 | zerocopy-derive | 0.8.57 | BSD-2-Clause OR Apache-2.0 OR MIT | - | crates.io | build-time proc-macro | LB-04 | no | A | A | A | - | [github.com/google/zerocopy](https://github.com/google/zerocopy) |


## cargo-deny 0.20.2（ADR-0027 D3 指定的工具）

配置：仓库根 `deny.toml`（ADR-0015 允许清单；GPL / AGPL / SSPL 不在名单内）。命令（探针在仓库外，`--config` 指向仓库根）：

```powershell
cargo deny --manifest-path <probe>/Cargo.toml --config <repo>/deny.toml check licenses advisories bans sources
```

结果（2026-09-23）：

| 检查 | 结果 |
| --- | --- |
| `licenses` | **ok**（270 包；唯一告警是 allow-list 里的 `MPL-2.0` 未被命中） |
| `bans` | **ok**（探针；仓库自身亦 `bans ok`，`wildcards` 对 `{ workspace = true }` 放宽后） |
| `sources` | **ok** |
| **`advisories`** | **FAILED** |

**advisories 的两条（这是本轮的阻塞发现，与许可证无关）**：

| 条目 | ID | 状态 |
| --- | --- | --- |
| `rustybuzz` 0.20.1（DC-17 指定的唯一 shaping 引擎） | **RUSTSEC-2026-0206** | **unmaintained**，且 “No safe upgrade is available”（最新即 0.20.1） |
| `ttf-parser` 0.25.1（rustybuzz 的字体解析依赖，也经 `ab_glyph → winit` 进入） | **RUSTSEC-2026-0192** | **unmaintained**，公告建议替代 `skrifa`（fontations） |

**结论**：**ADR-0027 D3 未满足，状态保持 Proposed**——许可证一侧通过，**公告一侧不通过**。依赖准入**仍未解除**；这条在依赖进入 workspace **之前**被采证抓到，正是 D3 存在的意义。处置见 `ADR-0031`（unmaintained shaping stack）。

