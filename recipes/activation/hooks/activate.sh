# Undo an outer target's activation before taking the next snapshot.
. "$CONDA_PREFIX/etc/conda/deactivate.d/000-pixi-cross.sh"
_pixi_cross_hook=$("$CONDA_PREFIX/bin/pixi-cross" activate start) || exit $?
eval "$_pixi_cross_hook"
unset _pixi_cross_hook
