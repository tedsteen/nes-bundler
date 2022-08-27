#!/bin/bash
# Where to find the config, defaults to config
CONF_DIR=${1:-config}

# Pack the config
zip -jT bundle.zip $CONF_DIR/config.yaml $CONF_DIR/rom.nes
