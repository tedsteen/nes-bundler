#!/bin/bash
VERSION=$1
#if [[ $(git branch --show-current) == "master" ]]; then
    cargo bump -g $VERSION
    exit 0
#else
#    >&2 echo "Can only release from master"
#fi
echo "USAGE:"
echo "    $0 [<version> | major | minor | patch]"
