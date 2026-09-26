# Third-party licences

Every release archive carries `THIRD-PARTY-NOTICES.txt` beside `LICENSE`: the licence of each
crate compiled into the binary, the NOTICE files those crates carry, and the licences and notices
of ONNX Runtime and, in the Windows archive, DirectML. `tools/release/notices.mjs` writes it for
each archive's target and GPU feature from `Cargo.lock`, and the release checks it against the
archive it is in (`.github/release-process.md` says how).

## The crates

`about.toml` is what cargo-about accepts: an ordered list of licences, and a clarification for
each crate whose licence text cargo-about cannot find or read on its own, naming the file that
is its licence and that file's SHA-256. The notices are refused, in CI on every pull request and
again at release, for a crate under a licence the list does not accept, for a clarification whose
file has changed, and for a crate whose only text would be SPDX's template of a licence that
names a copyright holder. What to do about any of these is in the message each one prints.

## The texts no crate carries

Kept byte for byte as their publishers ship them; `.gitattributes` stops Git changing their line
endings, so each digest below is the publisher's file. The notices file drops the byte order mark
and the carriage returns.

| Component | Version | In the archive | Files here | Taken from |
|---|---|---|---|---|
| ONNX Runtime | 1.28.0 | linked into the binary, on every platform, from the prebuilt library the `ort-sys` 2.0.0-rc.13 build downloads (`ms@1.28.0` in its table) | `onnxruntime/LICENSE`, `onnxruntime/ThirdPartyNotices.txt` | https://github.com/microsoft/onnxruntime at tag `v1.28.0` (commit `da9b5e364c465de65c49d91e696cd6485270757f`) |
| DirectML | 1.15.4 | `DirectML.dll` beside the Windows binary, byte for byte `bin/x64-win/DirectML.dll` of the package | `directml/LICENSE.txt`, `directml/LICENSE-CODE.txt`, `directml/ThirdPartyNotices.txt` | https://www.nuget.org/packages/Microsoft.AI.DirectML/1.15.4, the package root |

| File | SHA-256 |
|---|---|
| `onnxruntime/LICENSE` | `2f07c72751aed99790b8a4869cf2311df85a860b22ded05fa22803587a48922c` |
| `onnxruntime/ThirdPartyNotices.txt` | `0e07b95f3a8d6230037707c5c4a2b554d12c4cb67369669ac255635528ffcee2` |
| `directml/LICENSE.txt` | `a05138e3a085ff60a44881eedfa58dccb03ecc1d7b1f6ae888418e8c2fec4b8d` |
| `directml/LICENSE-CODE.txt` | `903df5512f7d02609fed0c780a9b704f5a3eeb6e4d84ebe42a29845c81899a3c` |
| `directml/ThirdPartyNotices.txt` | `2c95795c13ff48a58b6ed916f37901c23d964b5d9d601af422f17ad2172e7950` |
| `microsoft.ai.directml.1.15.4.nupkg` they came from | `4e7cb7ddce8cf837a7a75dc029209b520ca0101470fcdf275c1f49736a3615b9` |
| `DirectML.dll` in the Windows archive | `9c9e6d822561c6c41b90e6994b3e8857cf1d66dbfb1e0c4c799c7c89b4e92da1` |

The package's README says `LICENSE.txt` applies to everything under `bin/`, which is the DLL,
and `LICENSE-CODE.txt` (MIT) to its headers, which ONNX Runtime's DirectML support is compiled
against.

`notices.mjs` refuses an `ort-sys` whose table names a different ONNX Runtime, and the release
refuses a `DirectML.dll` with a different digest. Either means a new version: replace the files
here with that version's, and the versions and digests in `ONNX_RUNTIME` or `DIRECTML` in
`notices.mjs` and in this file.

## What DirectML's licence asks

`LICENSE.txt` is Microsoft's licence for the DirectML redistributable. Its duties on whoever
passes the DLL on, quoted:

- 1(a): "you may install and use any number of copies of the software, and solely for use on
  Windows and Xbox. You may copy and distribute the software (i.e. make available for third
  parties) in applications and services you develop in the build with Machine Learning tools and
  frameworks, and/or games that run on Windows and Xbox." The DLL ships only in the Windows
  archive, beside the application that loads it.
- 3(e): no right to "share, publish, distribute, or lease the software, provide the software as a
  stand-alone offering for others to use, or transfer the software or this agreement to any third
  party", "except as expressly stated in Section 1". It is never attached to a release on its
  own.
- 3(c): no right to "remove, minimize, block, or modify any notices of Microsoft or its suppliers
  in the software". It ships unmodified, which its digest shows.
- 1(c): "The software may include third party components with separate legal notices or governed
  by other agreements, as may be described in the ThirdPartyNotices file(s) accompanying the
  software." `ThirdPartyNotices.txt` travels with it; the BSD licence of the Zstandard code it
  lists asks exactly that of a binary.
- 2(a): "If you use these features to enable data collection in your applications, you must
  comply with applicable law". SteamGauge enables none.
- 4: "You must comply with all domestic and international export laws and regulations that apply
  to the software".

Nothing in it requires its own text to accompany the DLL. It travels anyway, so whoever receives
the archive knows the terms the DLL is under, among them that it may not be passed on by itself.
