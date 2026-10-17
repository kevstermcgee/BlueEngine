# Source from the user's shell startup after configuring the reviewed mount.
# Keep an explicit task-specific Cargo target.
if [ -z "${CARGO_TARGET_DIR:-}" ]; then
    export CARGO_TARGET_DIR=/srv/storage/cache/cargo/default
fi
