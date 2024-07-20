#!/bin/bash

LOCAL_WORKING_DIR=$(exec pwd)
CARGO_DIR="../rust"

echo "Setting up environment on cluster."

compile() {
  echo "Compiling the project"
  cd ${CARGO_DIR} || exit

  cargo build --release

  cd "${LOCAL_WORKING_DIR}" || exit
  cp ${CARGO_DIR}/target/release/microbenchmarks-async ./microbenchmarks
}

zip_folder() {

 rm -f "${LOCAL_WORKING_DIR}/$1.zip"

 echo "Zipping the $1 directory"

 zip -r -qq "${LOCAL_WORKING_DIR}/$1.zip" "./$1"
}

compile

zip_folder "config"
zip_folder "ca-root"

ansible-playbook -i hosts setup-env.yml

#ansible-playbook -i hosts run-experiment.yml