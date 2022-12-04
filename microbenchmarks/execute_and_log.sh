#!/bin/bash

RESULT_FOLDER="./${1}/${2}"

TO_RUN="servers"

if [[ $# -ge 3 ]]; then
    TO_RUN="$3"
fi

ulimit -n 100000

rm -rf "$RESULT_FOLDER" && mkdir -p "$RESULT_FOLDER" && ./run "$TO_RUN" | tee "$RESULT_FOLDER"/log.txt