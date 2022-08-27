#!/bin/bash
VERSION=$1
if [[ $(git branch --show-current) == "master" ]]; then
    git tag -a $VERSION -m"Release ${VERSION}" $2
    git push origin $VERSION $2
else
    >&2 echo "Can only release from master"
fi