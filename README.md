<div align="center">
  <img src=".github/assets/scarab.png" alt="Scarab">
</div>

Scarab builds portable music libraries from lossless collections. Using deterministic, file-based profiles, Scarab makes library selection and encoding behavior explicit, reproducible, and easy to adjust.

Scarab does **not** modify your original files.

## 🪲 why? 🪲

Lossless audio is an excellent format for keeping a master music collection, and storing a large lossless library at home is relatively easy with modern storage technology. Portable devices have tighter storage constraints, though, and carrying the same collection in FLAC on a phone is often much less practical.

Fortunately, audio encoding technology has gotten really good. Codecs such as [Opus](https://opus-codec.org/) can reduce music to a fraction of its lossless size while retaining very high listening quality at bitrates practical for portable storage.

Scarab builds smaller libraries from your lossless library. A TOML build profile makes the process deterministic, adjustable, and explicit: which collections are included, how audio is encoded, what size or bitrate to target, collection- and track-specific exceptions to apply, and which other file types are brought into the build.

### 🪲💽 collections 🪲

Collections let you group music from your library in many different ways.

A neatly organized album in a single directory can simply be one collection. A messy folder containing several albums can be split up using metadata such as album titles or artist names, and releases spread across multiple directories can be brought back together as one collection.

Collections do not have to correspond to albums. You can make collections for things like an artist's appearances across compilations, grouping multiple albums together, and more.

The idea of collections is to allow you to describe your desired output library without having to organize your source library around Scarab.

## 🪲 status 🪲

Scarab is under early development and is currently created and maintained by one person.

## 🪲 license 🪲

Scarab source code is licensed under the Mozilla Public License 2.0.

Test audio fixtures are independently licensed under CC0. See `tests/fixtures/library/README.md` for source attribution and more.
