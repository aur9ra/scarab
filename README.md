<div align="center">
  <img src=".github/assets/scarab.png" alt="Scarab">
</div>

Scarab builds portable music libraries from lossless collections. Using deterministic, file-based profiles, Scarab makes library selection and encoding behavior explicit, reproducible, and easy to adjust.

Scarab does **not** modify your original files.

## 🪲 why? 🪲

Lossless audio is an excellent format for keeping a master music collection, and storing a large lossless library at home is relatively easy with modern storage technology. Portable devices have tighter storage constraints, though, and carrying the same collection in FLAC on a phone is often much less practical.

Fortunately, audio encoding technology has gotten really good. Codecs such as [Opus](https://opus-codec.org/) can reduce music to a fraction of its lossless size while retaining very high listening quality at bitrates practical for portable storage.

Scarab builds smaller libraries from your lossless library. A TOML build profile makes the process deterministic, adjustable, and explicit: which albums are included, how audio is encoded, what size or bitrate to target, album- and track-specific exceptions to apply, and which other file types are brought into the build.

## 🪲 status 🪲

Scarab is under early development, and is created/maintained by one person. However, the intended behavior surface is intentionally low: stability is (hopefully) expectable reasonably soon.

## 🪲 license 🪲

Scarab source code is licensed under the Mozilla Public License 2.0.

Test audio fixtures are independently licensed under CC0. See `tests/fixtures/library/README.md` for source attribution and more.
