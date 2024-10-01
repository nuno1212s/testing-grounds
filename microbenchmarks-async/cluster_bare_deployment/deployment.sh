#!/bin/bash

LOCAL_WORKING_DIR=$(exec pwd)
CARGO_DIR="../rust"

echo "Setting up environment on cluster."

PROFILE="profiling"

compile() {
  echo "Compiling the project"
  cd ${CARGO_DIR} || exit

  export RUSTFLAGS="-C target-cpu=native"
  cargo build --profile ${PROFILE}

  cd "${LOCAL_WORKING_DIR}" || exit
  rm ./microbenchmarks
  cp ${CARGO_DIR}/target/${PROFILE}/microbenchmarks-async ./microbenchmarks
}

zip_folder() {

 rm -f "${LOCAL_WORKING_DIR}/$1.zip"

 echo "Zipping the $1 directory"

 zip -r -qq "${LOCAL_WORKING_DIR}/$1.zip" "./$1"
}

tar_folder() {

 rm -f "${LOCAL_WORKING_DIR}/$1.tar.gz"

 echo "Tarring the $1 directory"

 tar -czf "${LOCAL_WORKING_DIR}/$1.tar.gz" "./$1"
}

compile

zip_folder "config_clients"
zip_folder "config_replicas"
zip_folder "ca-root"

tar_folder "config_clients"
tar_folder "config_replicas"
tar_folder "ca-root"

ansible-playbook -i "$1" setup-env.yml
ansible-playbook -i "$1" setup-clients-env.yml
ansible-playbook -i "$1" setup-replicas-env.yml
#ansible-playbook -i hosts.yml run.yml

#ansible-playbook -i hosts run-experiment.yml