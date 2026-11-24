if [[ ${PIXI_CROSS_CHANGED+x} ]]; then
    _pixi_cross_hook=$("$CONDA_PREFIX/bin/pixi-cross" activate restore) || exit $?
    eval "$_pixi_cross_hook"
    unset _pixi_cross_hook
fi
