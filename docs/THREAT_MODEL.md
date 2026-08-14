# Filesystem threat model

The loader treats a caller-selected location as an ambient authority boundary,
not every pathname below it. This design protects local CLI use from accidental
or concurrent descendant pathname redirection; it is not an integrity system
against a privileged actor that can alter an already-open inode invisibly.

## Resolution rules

`--data-dir`, explicit `--scenario-dir`, the parent of an explicitly supplied
document path, and the legacy-derived scenario directory may each be followed
once as the selected ambient root. The resulting directory descriptor is pinned
and later pathname resolution does not re-use the ambient path. This preserves
intentional selected-root symlink support.

All descendants use descriptor-relative operations with no-follow semantics.
Final JSON symlinks, catalog child-directory symlinks, FIFOs, devices, sockets,
and directories masquerading as `.json` entries are rejected. An ambient-path
replacement after pinning cannot redirect later descendant authority.

## Catalog transaction

Catalog loading pins one data-root descriptor, inspects `rulesets` and
`rewards` without following them, opens both child descriptors before reading
either catalog, and reads candidates only through the already-pinned child
descriptors. It records and checks device/inode/type plus relevant metadata,
re-enumerates both child snapshots, and rechecks the root and child identities
before publication. A detected replacement or observable in-place mutation
fails closed as catalog I/O/path policy, normally with
`catalog_generation_changed`; no partial or mixed ruleset/reward catalog is
published.

Schema-v2 and schema-v3 rulesets/reward schedules are loaded and validated
inside this same transaction. There is no profile-specific filesystem loader.
An invalid or duplicate unreferenced v3 document rejects the complete catalog,
and bundle compilation rejects mixed scenario/ruleset/reward profiles.

The loader limits documents to 1 MiB, JSON depth to 64, inspected entries to
512, and retained JSON candidates to 256. Candidate names are sorted by raw
Unix bytes for deterministic behavior, including non-UTF-8 names.

Metadata comparisons detect observable mutation but are not cryptographic
integrity and cannot defeat an attacker able to modify an opened inode without
observable metadata changes. The implementation deliberately does not use
`canonicalize()` for authority, polling, sleeps, filesystem watchers, directory
hashing, `/proc/self/fd`, unsafe code, or an insecure fallback.

Secure descriptor-relative loading is available on Linux and Android. Other
targets fail closed with a path-policy error; they do not fall back to ordinary
symlink-following file opens.

V3 source IDs, labels, references, authority literals, banner IDs, and
probability values are data only. They never influence file lookup, catalog
enumeration, URL access, or security decisions. Provenance references remain
inert even when they resemble paths or URLs.
