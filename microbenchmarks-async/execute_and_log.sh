    #!/bin/bash

    RESULT_FOLDER=$1

    TO_RUN="servers"

    if [[ $# -ge 2 ]]; then
        TO_RUN="$2"
    fi

    rm -rf "$RESULT_FOLDER" && mkdir "$RESULT_FOLDER" && ./run "$TO_RUN" | tee "$RESULT_FOLDER"/log.txt