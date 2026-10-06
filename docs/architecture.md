# Scarab’s design and architectural boundaries

## Purpose and source authority

Scarab is intended as a Rust CLI for reproducibly building a compressed music library from an authoritative, read-only source. Its output is a disposable derivative that can be rebuilt from that source and a build specification, not another authoritative collection. Scarab **must absolutely not, under any circumstances** organize, repair or perform any other modifications upon the provided library source.

Scarab is not a sync tool, organizer, downloader, metadata-repair tool, device manager, general ffmpeg tool, or image-processing tool. Reproducibility concerns the inputs and decisions used for a build, not a guarantee of byte-identical output across different tool versions.

## Observation and interpretation

Configuration validation is limited to validating the internal consistency of a supplied build specification. It does not establish filesystem validity, metadata matches, or whether output can be chosen.

Directory resolution and filesystem discovery are observations, not lasting validity guarantees. Resolving a path does not establish containment within `source_root`, and discovery does not establish a stable filesystem snapshot.

Source-audio classification is purely lexical: it classifies a pathname, not the existence, readability, or media contents of a file. Probing observes media information for one supplied pathname. It does not determine collection membership or whether/how the file will be included in the built library.

An observed source file’s reporting-scope association does not by itself establish collection membership. Membership interprets reporting-scope associations together with supplied metadata predicates. Output policy separately determines what to emit and how that output is produced.

A failed or incomplete observation does not by itself establish a negative result. It does not prove that a referenced path is absent from the filesystem, that an observed source file is not a member of a collection, or that a required scope would report no files if discovery completed. Resolution, discovery, classification, and probing remain independently usable capabilities, and no mandatory inventory gateway or execution pipeline is required.

## Build specifications and identity

`LibraryBuildSpec` is a validated declarative specification, not mutable execution state. Nested values detached from a validated `LibraryBuildSpec` do not carry its aggregate validation guarantees independently.

Collection handles are exact configuration identities and rule targets, not album metadata identities such as name or artist. Each supplied exact metadata value is distinct from an omitted metadata predicate, including an explicitly supplied empty or whitespace-only value.

Configuration ordering does not establish policy precedence.

## Paths, scopes, and coverage

Directory-selector resolution interprets one configured selector independently of collection identity. Scope preparation produces filesystem scopes on behalf of collection declarations. Scope coverage, membership evaluation, and output policy are distinct concerns. A collection declaration supplies membership criteria, while collection rules target handles to override output policy.

`source_root` is an anchor for relative explicit selectors and supplies the default scope for collection declarations that omit explicit selectors. It is not a containment boundary or a security boundary. Explicit selectors and the implicit default scope have distinct meanings, and failure to resolve an explicit selector does not make that selector equivalent to omission or activate default-scope fallback.

Logical scopes remain distinct even when their `PathBuf` roots compare equal. Observed source file pathnames are compared using native `Path` equality. This is pathname identity, not physical-file identity. A complete observed source file inventory requires complete coverage of every required scope, including scopes that report no files.

Work sharing during coverage cannot hide a failure in required coverage or erase a logical coverage obligation. Lexical ancestry cannot substitute for required coverage. Inventory is an observation of filesystem coverage, not a stable filesystem snapshot.

## Membership and output policy

Membership interprets an observed source file’s reporting-scope associations together with every exact metadata predicate when supplied. A collection’s configured directory scopes contribute a union of observed source files. Directory-only membership includes the files in that union recognized as source audio. An observed source file may belong to several configured collections. This is valid and does not itself create ambiguity or an ownership conflict. No first-wins or declaration-order rule assigns a file to a collection.

Metadata-selector comparison is symmetric. Comparison removes all trailing U+0000s from each supplied value, NFC-normalizes both results, and compares them in full with case-sensitive equality. Whitespace, case, punctuation, embedded and leading U+0000, and compatibility-distinct Unicode are not removed. Matching under this rule does not redefine original or stored value identity. Comparison never rewrites either supplied representation.

`MetadataSelectorObservation`: Records complete, successful results for album names, album artists, and track artists. Each family (stored as `BTreeSet`s) is a set of raw strings under exact equality. Exact duplicates collapse, but comparison-equivalent strings remain distinct. An empty family means no value was observed, not an empty or NUL-only value. The type cannot enforce completeness. Extraction failure, incomplete extraction, and unavailable metadata remain outside the observation.

Omitted predicate families impose no constraints. Supplied alternatives combine by OR under metadata-selector comparison, and supplied families combine by AND. A supplied family with no alternatives is unsatisfiable. Each family is evaluated independently, so values in one family cannot satisfy another family’s constraint. Alternative order and repetition do not affect evaluation.

Global -> collection -> track overrides are explicit policy. Conflicting rules at the same precedence level block rather than being resolved by declaration order. Missing or ambiguous encoding requirements block encoding. Core logic does not prompt on stdin to resolve such requirements.

Positive source-audio classification reserves the source-audio role for that pathname. Probe failure, membership non-match, or output exclusion does not reopen auxiliary-file handling. Auxiliary-file selection does not establish collection membership or expand collection scopes.
