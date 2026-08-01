# Shared bench Makefile — called by testing-grounds/Makefile as:
#   make -f testing-grounds/bench/Makefile <target> \
#        GLOBAL_BENCH_DIR=/abs/path/testing-grounds/bench \
#        PROJECT_BENCH_DIR=/abs/path/<project>/bench

ifndef GLOBAL_BENCH_DIR
$(error GLOBAL_BENCH_DIR is not set. Invoke via testing-grounds/Makefile: make <project> <target>)
endif

# Cleanup targets that operate on the shared/global bench dir and therefore do not
# require a project to be named. Everything else needs PROJECT_BENCH_DIR.
PROJECT_FREE_TARGETS := clean clean-docker distclean help
NEEDS_PROJECT        := $(filter-out $(PROJECT_FREE_TARGETS),$(MAKECMDGOALS))

ifndef PROJECT_BENCH_DIR
ifneq ($(NEEDS_PROJECT),)
$(error PROJECT_BENCH_DIR is not set. Invoke via testing-grounds/Makefile: make <project> <target>)
endif
endif

include $(GLOBAL_BENCH_DIR)/bench.env
-include $(PROJECT_BENCH_DIR)/bench.env
export

SED_I      := $(if $(filter Darwin,$(shell uname)),sed -i '',sed -i)
DOCKER     ?= $(if $(shell command -v podman 2>/dev/null),podman,docker)
COMPOSE    ?= $(if $(shell command -v podman-compose 2>/dev/null),podman-compose,docker-compose)

BENCH_DIR       := $(PROJECT_BENCH_DIR)
SCRIPTS         := $(GLOBAL_BENCH_DIR)/scripts
PLAYBOOKS       := $(GLOBAL_BENCH_DIR)/playbooks
# Single shared generated/ for all projects — lives in the global bench dir.
GENERATED       := $(GLOBAL_BENCH_DIR)/generated
HOSTS           := $(PROJECT_BENCH_DIR)/hosts.yml
CONFIG_BASE     := $(GLOBAL_BENCH_DIR)/config-base

# Dockerfile and build context are always fixed relative to the shared bench dir.
DOCKERFILE_ABS  := $(GLOBAL_BENCH_DIR)/Dockerfile
BUILD_CTX_ABS   := $(abspath $(GLOBAL_BENCH_DIR)/../..)
RUST_SRC_ABS    := $(abspath $(PROJECT_BENCH_DIR)/$(RUST_SRC_RELPATH))
# APP_SOURCE_DIR: RUST_SRC_ABS expressed relative to the Atlas repo root (build context).
APP_SOURCE_DIR  := $(patsubst $(BUILD_CTX_ABS)/%,%,$(RUST_SRC_ABS))

.PHONY: local stop-local logs-local \
        remote-docker stop-remote-docker \
        remote-bare stop-remote-bare build-binary \
        gen-configs gen-ca-root regen-ca-root \
        clean clean-docker clean-cargo distclean help

# ── Help ─────────────────────────────────────────────────────────────────────────
help:
	@echo "Usage: make <project> <target> [VAR=value ...]"
	@echo ""
	@echo "  local              Build image and run N_REPLICAS replicas + N_CLIENTS clients locally"
	@echo "  stop-local         Stop and remove local containers"
	@echo "  logs-local         Follow logs from local containers"
	@echo ""
	@echo "  remote-docker      Deploy to SSH cluster using Docker (Ansible)"
	@echo "  stop-remote-docker Stop remote Docker deployment"
	@echo ""
	@echo "  remote-bare        Compile binary and deploy to SSH cluster as bare processes (Ansible)"
	@echo "  stop-remote-bare   Kill remote bare processes"
	@echo ""
	@echo "  build-binary       Compile the Rust binary"
	@echo "  gen-ca-root        Generate ca-root if not already present"
	@echo "  regen-ca-root      Force regeneration of ca-root certificates"
	@echo ""
	@echo "  clean              Remove the shared generated/ dir (configs, ca-root, logs). No project needed."
	@echo "  clean-docker       Remove local Docker artifacts (atlas_network, containers, influxdb volume;"
	@echo "                     also the project image when a project is named)"
	@echo "  clean-cargo        cargo clean the named project's Rust target/ (requires a project)"
	@echo "  distclean          clean-docker + clean-cargo + clean (full wipe)"
	@echo ""
	@echo "Key variables (override on command line or in project bench.env):"
	@echo "  N_REPLICAS=$(N_REPLICAS)  N_CLIENTS=$(N_CLIENTS) (per machine)  N_CLIENT_MACHINES=$(N_CLIENT_MACHINES)"
	@echo "  Total logical clients = N_CLIENTS x N_CLIENT_MACHINES = $$(( $(N_CLIENTS) * $(N_CLIENT_MACHINES) ))"
	@echo "  CONCURRENT_RQS=$(CONCURRENT_RQS)  OPS_NUMBER=$(OPS_NUMBER)"
	@echo ""
	@echo "Edit $(CONFIG_BASE)/common/influx_db.toml to change InfluxDB target."
	@echo "Generated output: $(GENERATED)"

# ── CA-root generation ────────────────────────────────────────────────────────────
# Generates PKI certificates for N_REPLICAS servers and N_CLIENTS clients.
# Skips if already present. Use regen-ca-root to force after changing cluster size.
gen-ca-root:
	@mkdir -p $(GENERATED)
	@bash $(SCRIPTS)/gen-ca-root.sh

regen-ca-root:
	@rm -rf $(GENERATED)/ca-root
	@bash $(SCRIPTS)/gen-ca-root.sh

# ── Config generation ─────────────────────────────────────────────────────────────
# Copies config-base templates from shared bench into generated/, substituting
# benchmark placeholders. Generates ca-root first if not present.
gen-configs: gen-ca-root
	@mkdir -p $(GENERATED)/config-replicas $(GENERATED)/config-clients $(GENERATED)/logs
	@cp $(CONFIG_BASE)/common/*.toml $(GENERATED)/config-replicas/
	@cp $(CONFIG_BASE)/common/*.toml $(GENERATED)/config-clients/
	@cp $(CONFIG_BASE)/replica/*.toml $(GENERATED)/config-replicas/
	@cp $(CONFIG_BASE)/client/*.toml  $(GENERATED)/config-clients/
	@$(SED_I) "s|CONCURRENT_RQS_PLACEHOLDER|$(CONCURRENT_RQS)|g" \
	    $(GENERATED)/config-replicas/benchmark_config.toml \
	    $(GENERATED)/config-clients/benchmark_config.toml
	@$(SED_I) "s|OPS_NUMBER_PLACEHOLDER|$(OPS_NUMBER)|g" \
	    $(GENERATED)/config-replicas/benchmark_config.toml \
	    $(GENERATED)/config-clients/benchmark_config.toml
	@$(SED_I) "s|REQUEST_SLEEP_MILLIS_PLACEHOLDER|$(REQUEST_SLEEP_MILLIS)|g" \
	    $(GENERATED)/config-replicas/benchmark_config.toml \
	    $(GENERATED)/config-clients/benchmark_config.toml
	@$(SED_I) "s|CLIENTS_TO_RUN_PLACEHOLDER|$(N_CLIENTS)|g" \
	    $(GENERATED)/config-clients/client_config.toml
	@echo "Configs generated in $(GENERATED)/config-{replicas,clients}/"

# ── Local Docker Compose ──────────────────────────────────────────────────────────
local: gen-configs
	@bash $(SCRIPTS)/gen-nodes-toml.sh local $(N_REPLICAS) $(N_CLIENT_MACHINES)
	@cp $(GENERATED)/nodes.toml $(GENERATED)/config-replicas/nodes.toml
	@cp $(GENERATED)/nodes.toml $(GENERATED)/config-clients/nodes.toml
	@bash $(SCRIPTS)/gen-local-compose.sh
	$(DOCKER) build -t $(IMAGE_NAME) \
	    --build-arg APP_NAME=$(BINARY_NAME) \
	    --build-arg APP_SOURCE_DIR=$(APP_SOURCE_DIR) \
	    -f $(DOCKERFILE_ABS) $(BUILD_CTX_ABS)
	$(DOCKER) network create atlas_network 2>/dev/null || true
	@bash -c 'trap "$(COMPOSE) -f $(GENERATED)/local-compose.yml down; sleep 1; $(DOCKER) network rm atlas_network 2>/dev/null || true" EXIT; $(COMPOSE) -f $(GENERATED)/local-compose.yml up' || true

stop-local:
	$(COMPOSE) -f $(GENERATED)/local-compose.yml down
	$(DOCKER) network rm atlas_network 2>/dev/null || true

logs-local:
	$(COMPOSE) -f $(GENERATED)/local-compose.yml logs -f

# ── Remote Docker ─────────────────────────────────────────────────────────────────
remote-docker: gen-configs
	@bash $(SCRIPTS)/gen-nodes-toml.sh remote $(N_REPLICAS) $(N_CLIENT_MACHINES)
	@cp $(GENERATED)/nodes.toml $(GENERATED)/config-replicas/nodes.toml
	@cp $(GENERATED)/nodes.toml $(GENERATED)/config-clients/nodes.toml
	@bash $(SCRIPTS)/gen-remote-compose.sh
	ansible-playbook -i $(HOSTS) $(PLAYBOOKS)/remote-docker-deploy.yml \
	    -e "generated_dir=$(GENERATED) remote_workdir=$(REMOTE_WORKDIR)"

stop-remote-docker:
	ansible-playbook -i $(HOSTS) $(PLAYBOOKS)/remote-docker-stop.yml \
	    -e "remote_workdir=$(REMOTE_WORKDIR)"

# ── Remote Bare ───────────────────────────────────────────────────────────────────
remote-bare: gen-configs build-binary
	@bash $(SCRIPTS)/gen-nodes-toml.sh remote $(N_REPLICAS) $(N_CLIENT_MACHINES)
	@cp $(GENERATED)/nodes.toml $(GENERATED)/config-replicas/nodes.toml
	@cp $(GENERATED)/nodes.toml $(GENERATED)/config-clients/nodes.toml
	@bash $(SCRIPTS)/gen-remote-envs.sh
	ansible-playbook -i $(HOSTS) $(PLAYBOOKS)/remote-bare-deploy.yml \
	    -e "generated_dir=$(GENERATED) remote_workdir=$(REMOTE_WORKDIR) \
	        binary_name=$(BINARY_NAME) rust_log='$(RUST_LOG)' \
	        request_size=$(REQUEST_SIZE) reply_size=$(REPLY_SIZE) state_size=$(STATE_SIZE)"

stop-remote-bare:
	ansible-playbook -i $(HOSTS) $(PLAYBOOKS)/remote-bare-stop.yml \
	    -e "binary_name=$(BINARY_NAME)"

build-binary:
	@echo "Compiling $(BINARY_NAME)..."
	cd $(RUST_SRC_ABS) && RUSTFLAGS="-C target-cpu=native" cargo build --release
	@mkdir -p $(GENERATED)
	@cp $(RUST_SRC_ABS)/target/release/$(BINARY_NAME) $(GENERATED)/$(BINARY_NAME)
	@echo "Binary copied to $(GENERATED)/$(BINARY_NAME)"

# ── Cleanup ───────────────────────────────────────────────────────────────────────
# clean removes only the shared generated/ dir — project-independent.
clean:
	rm -rf $(GENERATED)
	@echo "Cleaned $(GENERATED)"

# clean-docker tears down what `local` creates. The generated compose file already
# knows every container/network/volume name, so `down -v --rmi local` is the tidy
# path; the standalone network rm covers the network created directly by `local`.
# The project image is only removed when IMAGE_NAME is known (i.e. a project is named).
clean-docker:
	-@[ -f $(GENERATED)/local-compose.yml ] && \
	    $(COMPOSE) -f $(GENERATED)/local-compose.yml down -v --rmi local 2>/dev/null || true
	-$(DOCKER) network rm atlas_network 2>/dev/null || true
	$(if $(IMAGE_NAME),-$(DOCKER) rmi $(IMAGE_NAME) 2>/dev/null || true)
	@echo "Removed local Docker artifacts (network, containers, volume$(if $(IMAGE_NAME), + image $(IMAGE_NAME)))"

# clean-cargo purges the named project's Rust build output. Needs a project — without
# one PROJECT_BENCH_DIR is empty and RUST_SRC_ABS collapses to /, so guard on it.
clean-cargo:
	@if [ -n "$(PROJECT_BENCH_DIR)" ] && [ -d "$(RUST_SRC_ABS)" ]; then \
	    echo "cargo clean in $(RUST_SRC_ABS)"; cd $(RUST_SRC_ABS) && cargo clean; \
	 else echo "clean-cargo: no project source dir (name a project to purge target/)"; fi

# distclean does the heavy removals first (they read the compose file / source dir),
# then drops the shared generated/ dir last.
distclean: clean-docker clean-cargo
	rm -rf $(GENERATED)
	@echo "distclean complete"
