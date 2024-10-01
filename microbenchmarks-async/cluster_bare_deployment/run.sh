#!/bin/bash

. .env
. ".own_${1}.env"

if [ "$1" -eq 1000 ]; then
  mkdir profiler | exit 1
  AMDuProfCLI collect -o profiler ./microbenchmarks
else
  ./microbenchmarks
fi