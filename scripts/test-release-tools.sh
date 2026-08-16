#!/usr/bin/env bash
# Fast, side-effect-free checks for release helper syntax and policy invariants.
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
scripts=(
    audit.sh check-release-workflow.sh check-workflow-pins.sh package-release.sh
    test-release-tools.sh verify-release-archive.sh verify-shipped-data.sh
)
for script in "${scripts[@]}"; do
    bash -n "$repo_root/scripts/$script"
done

"$repo_root/scripts/check-workflow-pins.sh"
"$repo_root/scripts/check-release-workflow.sh"
"$repo_root/scripts/package-release.sh" --help >/dev/null 2>&1
"$repo_root/scripts/verify-release-archive.sh" --help >/dev/null 2>&1

command -v tar >/dev/null 2>&1 || { printf '%s\n' 'error: tar is required' >&2; exit 1; }
command -v truncate >/dev/null 2>&1 || { printf '%s\n' 'error: truncate is required' >&2; exit 1; }
command -v gzip >/dev/null 2>&1 || { printf '%s\n' 'error: gzip is required' >&2; exit 1; }
command -v cat >/dev/null 2>&1 || { printf '%s\n' 'error: cat is required' >&2; exit 1; }

test_root="$(mktemp -d)"
cleanup() {
    rm -rf -- "$test_root"
}
trap cleanup EXIT

verifier="$repo_root/scripts/verify-release-archive.sh"
runtime_root="ba-strategy-v0.3.0-x86_64-unknown-linux-gnu"
tar_plain=(
    tar --create --sort=name --mtime=@0 --owner=0 --group=0 --numeric-owner
)
tar_create=("${tar_plain[@]}" --gzip)

expect_rejection() {
    local archive="$1"
    local expected="$2"
    local output
    if output="$("$verifier" "$archive" 2>&1)"; then
        printf 'error: unsafe release fixture was accepted: %s\n' "$archive" >&2
        exit 1
    fi
    [[ "$output" == *"$expected"* ]] || {
        printf 'error: release fixture failed for the wrong reason: %s\n%s\n' "$archive" "$output" >&2
        exit 1
    }
}

case_root="$test_root/extra-root"
mkdir -p -- "$case_root/$runtime_root" "$case_root/unexpected-root"
printf '%s\n' 'unreviewed' > "$case_root/unexpected-root/payload"
chmod -R u=rwX,go=rX "$case_root"
"${tar_create[@]}" --file "$test_root/extra-root.tar.gz" \
    --directory "$case_root" "$runtime_root" unexpected-root
expect_rejection "$test_root/extra-root.tar.gz" "outside root"

case_root="$test_root/early-terminator"
mkdir -p -- "$case_root/$runtime_root"
chmod -R u=rwX,go=rX "$case_root"
"${tar_plain[@]}" --file "$test_root/early-first.tar" \
    --directory "$case_root" "$runtime_root"
printf '%s\n' 'phantom required file' > "$case_root/$runtime_root/README.md"
chmod -R u=rwX,go=rX "$case_root"
"${tar_plain[@]}" --file "$test_root/early-second.tar" \
    --directory "$case_root" "$runtime_root/README.md"
cat -- "$test_root/early-first.tar" "$test_root/early-second.tar" \
    > "$test_root/early-combined.tar"
gzip --stdout -- "$test_root/early-combined.tar" > "$test_root/early-terminator.tar.gz"
expect_rejection "$test_root/early-terminator.tar.gz" "entries after its end-of-archive marker"

case_root="$test_root/link"
mkdir -p -- "$case_root/$runtime_root"
ln -s -- /tmp "$case_root/$runtime_root/unsafe-link"
chmod -R u=rwX,go=rX "$case_root"
"${tar_create[@]}" --file "$test_root/link.tar.gz" --directory "$case_root" "$runtime_root"
expect_rejection "$test_root/link.tar.gz" "directory or regular file"

case_root="$test_root/hard-link"
mkdir -p -- "$case_root/$runtime_root"
printf '%s\n' 'same inode' > "$case_root/$runtime_root/original"
ln -- "$case_root/$runtime_root/original" "$case_root/$runtime_root/hard-link"
chmod -R u=rwX,go=rX "$case_root"
"${tar_create[@]}" --file "$test_root/hard-link.tar.gz" \
    --directory "$case_root" "$runtime_root"
expect_rejection "$test_root/hard-link.tar.gz" "directory or regular file"

case_root="$test_root/duplicate"
mkdir -p -- "$case_root/$runtime_root"
printf '%s\n' 'duplicate' > "$case_root/$runtime_root/duplicate"
chmod -R u=rwX,go=rX "$case_root"
"${tar_plain[@]}" --file "$test_root/duplicate.tar" \
    --directory "$case_root" "$runtime_root"
tar --append --file "$test_root/duplicate.tar" \
    --directory "$case_root" "$runtime_root/duplicate"
gzip --stdout -- "$test_root/duplicate.tar" > "$test_root/duplicate.tar.gz"
expect_rejection "$test_root/duplicate.tar.gz" "duplicate archive entry"

case_root="$test_root/oversized"
mkdir -p -- "$case_root/$runtime_root"
truncate --size "$((32 * 1024 * 1024 + 1))" "$case_root/$runtime_root/oversized"
chmod -R u=rwX,go=rX "$case_root"
"${tar_create[@]}" --file "$test_root/oversized.tar.gz" \
    --directory "$case_root" "$runtime_root"
expect_rejection "$test_root/oversized.tar.gz" "member exceeds maximum"

case_root="$test_root/decompression-bound"
mkdir -p -- "$case_root/$runtime_root"
truncate --size "$((66 * 1024 * 1024 + 1))" "$case_root/$runtime_root/compressed-bomb"
chmod -R u=rwX,go=rX "$case_root"
"${tar_create[@]}" --file "$test_root/decompression-bound.tar.gz" \
    --directory "$case_root" "$runtime_root"
expect_rejection "$test_root/decompression-bound.tar.gz" "decompressed archive stream exceeds maximum"

case_root="$test_root/many"
mkdir -p -- "$case_root/$runtime_root"
for ((index = 0; index < 2049; index++)); do
    : > "$case_root/$runtime_root/entry-$index"
done
chmod -R u=rwX,go=rX "$case_root"
"${tar_create[@]}" --file "$test_root/many.tar.gz" --directory "$case_root" "$runtime_root"
expect_rejection "$test_root/many.tar.gz" "invalid or exceeds maximum"

case_root="$test_root/checksum"
mkdir -p -- "$case_root/$runtime_root"
chmod -R u=rwX,go=rX "$case_root"
"${tar_create[@]}" --file "$test_root/checksum.tar.gz" \
    --directory "$case_root" "$runtime_root"
printf '%064d  %s\n' 0 "../../outside.tar.gz" > "$test_root/checksum.tar.gz.sha256"
expect_rejection "$test_root/checksum.tar.gz" "must name only the selected archive"

case_root="$test_root/checksum-link"
mkdir -p -- "$case_root/$runtime_root"
chmod -R u=rwX,go=rX "$case_root"
"${tar_create[@]}" --file "$test_root/checksum-link.tar.gz" \
    --directory "$case_root" "$runtime_root"
ln -s -- /dev/null "$test_root/checksum-link.tar.gz.sha256"
expect_rejection "$test_root/checksum-link.tar.gz" "non-symlink regular file"

if output="$(
    "$repo_root/scripts/package-release.sh" \
        --version 0.3.0 \
        --target ../../outside \
        --output-dir "$test_root/package-output" 2>&1
)"; then
    printf '%s\n' 'error: package helper accepted a path-like release target' >&2
    exit 1
fi
[[ "$output" == *"unsupported release target"* ]] || {
    printf 'error: unsafe release target failed for the wrong reason\n%s\n' "$output" >&2
    exit 1
}
