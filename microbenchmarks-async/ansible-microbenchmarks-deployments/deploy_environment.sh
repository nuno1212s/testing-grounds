#!/bin/bash

LOCAL_WORKING_DIR="./"
#LOCAL_CONFIG_DIR="${LOCAL_WORKING_DIR}/config"
LOCAL_CA_ROOT_DIR="${LOCAL_WORKING_DIR}/ca-root"
ANSIBLE_COMPOSE_GENERATOR="ansible-compose-generation"

echo "Setting up environment on cluster."

ansible-playbook -i hosts setup-env.yml

# shellcheck disable=SC2012
# shellcheck disable=SC2010

echo "Zipping the ca-root directory"

ANSIBLE_DIR=$(exec pwd)

cd ${LOCAL_WORKING_DIR} || exit

rm -f "${ANSIBLE_DIR}"/ca-root.zip

zip -r -qq "${ANSIBLE_DIR}"/ca-root.zip ./ca-root

cd "${ANSIBLE_DIR}" || exit

echo "Setting up configurations on the cluster."

ansible-playbook -i hosts setup-configs.yml

#REPLICA_COUNTS=(4 8 16 32)
#CLIENT_COUNTS=(1 10 100 1000)
#REQUEST_SIZE=(0 128 1024 4096)

REPLICA_COUNTS=(4)
CLIENT_COUNTS=(3 6 9)
REQUEST_SIZE=(0)

TEST_RUNS=1

echo "Preparing test executions with configurations:"
echo "Replica counts: ${REPLICA_COUNTS[*]}"
echo "Client counts: ${CLIENT_COUNTS[*]}"
echo "Request sizes: ${REQUEST_SIZE[*]}"
echo "Test runs: ${TEST_RUNS}"

for REQUEST_SIZE in "${REQUEST_SIZE[@]}"; do
  echo "Running experiment with request size ${REQUEST_SIZE}"
  for REPLICA_COUNT in "${REPLICA_COUNTS[@]}"; do
    echo "Running experiment for replica count: ${REPLICA_COUNT}"
    for CLIENT_COUNT in "${CLIENT_COUNTS[@]}"; do

      echo "Running experiment for replica count: ${REPLICA_COUNT}, client count: ${CLIENT_COUNT}, request size: ${REQUEST_SIZE}"

      rm -rf generated

      # Generate the docker compose files
      cd ${ANSIBLE_COMPOSE_GENERATOR} || exit

      echo "Generating docker compose files"

      cargo run --release -- --replica-count "${REPLICA_COUNT}" --client-count "${CLIENT_COUNT}" --request-size "${REQUEST_SIZE}" --reply-size "${REQUEST_SIZE}" round-robin

      cd "${ANSIBLE_DIR}" || exit

      echo "Deploying the files to the cluster"

      ansible-playbook -i hosts setup-docker-files.yml

      echo "Starting the experiment for ${TEST_RUNS} runs"

      for i in $(seq 1 ${TEST_RUNS}); do
        export INFLUX_EXTRA="R_${REPLICA_COUNT}_CLI_${CLIENT_COUNT}_REQ_${REQUEST_SIZE}_RUN_${i}"
        echo "Starting run ${i}"
        ansible-playbook -i hosts run-testing-scenario.yml
        echo "Finished run ${i}"
        #Run this separately so we wait for all clients to finish before stopping the replicas
        ansible-playbook -i hosts shutdown-replicas.yml
      done
    done
  done
done

echo "Finished test scenarios."
