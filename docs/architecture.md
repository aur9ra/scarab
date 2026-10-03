# Scarab’s design and architectural boundaries

## Purpose and source authority

Scarab is intended as a Rust CLI for reproducibly building a compressed music library from an authoritative, read-only source. Its output is a disposable derivative that can be rebuilt from that source and a build specification, not another authoritative collection. Scarab **must absolutely not, under any circumstances** organize, repair or perform any other modifications upon the provided library source.

Scarab is not a sync tool, organizer, downloader, metadata-repair tool, device manager, general ffmpeg tool, or image-processing tool. Reproducibility concerns the inputs and decisions used for a build, not a guarantee of byte-identical output across different tool versions.

## Observation and interpretation

Configuration validation is limited to validating the internal consistency of a supplied build specification. It does not establish filesystem validity, metadata matches, or whether output can be chosen.

Directory resolution and filesystem discovery are observations, not lasting validity guarantees. Resolving a path does not establish containment within `source_root`, and discovery does not establish a stable filesystem snapshot.

Source-audio classification is purely lexical: it classifies a pathname, not the existence, readability, or media contents of a file. Probing observes media information for one supplied pathname. It does not determine album membership or whether/how the file will be included in the built library.

A file’s association with a scope is not membership in an album. Membership interprets candidate scope together with supplied metadata predicates. Output policy separately determines what to emit and how that output is produced.

A failed or incomplete observation does not by itself establish a negative result. It does not prove that a referenced path is absent from the filesystem, that a candidate is not a member of an album, or that a required scope contains no candidates. Resolution, discovery, classification, and probing remain independently usable capabilities, and no mandatory inventory gateway or execution pipeline is required.

## Build specifications and identity

`LibraryBuildSpec` is a validated declarative specification, not mutable execution state. Nested values detached from a validated `LibraryBuildSpec` do not carry its aggregate validation guarantees independently.

Album handles are configuration identities and rule targets, not album metadata identities such as name or artist. Each supplied exact metadata value is distinct from an omitted metadata predicate, including an explicitly supplied empty or whitespace-only value.

Configuration ordering does not establish policy precedence.

## Paths, scopes, and coverage

`source_root` is an anchor for relative explicit selectors and supplies the default scope for albums that omit explicit selectors. It is not a containment boundary or a security boundary. Explicit selectors and the implicit default scope have distinct meanings, and failure to resolve an explicit selector does not make that selector equivalent to omission or activate default-scope fallback.

Logical scopes preserve why coverage is required even when two `PathBuf` scope roots compare equal. Candidate-path equality is not physical-file identity. A complete inventory requires complete coverage of every required scope, including scopes that contain no candidates.

Work sharing during coverage cannot hide a failure in required coverage or erase a logical coverage obligation. Lexical ancestry cannot substitute for required coverage. Inventory is an observation of filesystem coverage, not a stable filesystem snapshot.

## Membership and output policy

Membership combines the applicable candidate scope with every exact metadata predicate when supplied. Multiple configured directories contribute a candidate union, and directory-only membership is valid. A source pathname may belong to several configured collections; this is valid and does not itself create ambiguity or an ownership conflict. No first-wins or declaration-order rule assigns the pathname to a collection.

Global -> album -> track overrides are explicit policy. Conflicting rules at the same precedence level block rather than being resolved by declaration order. Missing or ambiguous encoding requirements block encoding. Core logic does not prompt on stdin to resolve such requirements.

Positive source-audio classification reserves the source-audio role for that pathname. Probe failure, membership non-match, or output exclusion does not reopen auxiliary-file handling. Auxiliary-file selection does not establish album membership or expand album scopes.
