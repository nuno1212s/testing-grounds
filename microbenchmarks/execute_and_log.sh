#!/bin/bash

RESULT_FOLDER="./results/${1}"

if [ ! -f $RESULT_FOLDER ]; then

    mkdir -p $RESULT_FOLDER

fi

TO_RUN="servers"

if [[ $# -ge 2 ]]; then
    TO_RUN="$2"
fi

ulimit -n 50000

rm -rf "$RESULT_FOLDER" && mkdir "$RESULT_FOLDER" && ./run "$TO_RUN" | tee "$RESULT_FOLDER"/log.txt