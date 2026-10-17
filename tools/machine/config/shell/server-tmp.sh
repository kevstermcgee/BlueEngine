# Source from the user's shell startup after configuring the reviewed mount.
# Keep an explicitly chosen alternative TMPDIR.
if [ -z "${TMPDIR:-}" ] || [ "$TMPDIR" = /tmp ] || [ "$TMPDIR" = "$HOME/.cache/server-tmp" ]; then
    export TMPDIR=/srv/storage/scratch/manual
fi
