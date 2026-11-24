_pixi_cross_hook=$(printf '%s\0' "${_pixi_cross_before[@]}" | "$CONDA_PREFIX/bin/pixi-cross" activate finish) || exit $?
eval "$_pixi_cross_hook"
unset _pixi_cross_before _pixi_cross_hook
