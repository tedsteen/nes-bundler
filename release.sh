#!/bin/bash
VERSION=$1
if [[ -z "$VERSION" ]]; then
    echo "Usage: $0 [major, minor, patch, <version>]"
else
    cargo release $VERSION --execute
fi
