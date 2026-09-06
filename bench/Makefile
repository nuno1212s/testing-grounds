# Shared bench Makefile — called by testing-grounds/Makefile as:
#   make -f testing-grounds/bench/Makefile <target> \
#        GLOBAL_BENCH_DIR=/abs/path/testing-grounds/bench \
#        PROJECT_BENCH_DIR=/abs/path/<project>/bench

ifndef GLOBAL_BENCH_DIR
$(error GLOBAL_BENCH_DIR is not set. Invoke via testing-grounds/Makefile: make <project> <target>)
endif
ifndef PROJECT_BENCH_DIR
$(error PROJECT_BENCH_DIR is not set. Invoke via testing-grounds/Makefile: make <project> <target>)
endif

include $(GLOBAL_BENCH_DIR)/bench.env
-include $(PROJECT_BENCH_DIR)/bench.env
export

SED_I      := $(if $(filter Darwin,$(shell uname)),sed -i '',sed -i)
DOCKER     ?= $(if $(shell command -v podman 2>/dev/null),podman,docker)
COMPOSE    ?= $(if $(shell command -v podman-compose 2>/dev/null),podman-compose,$(if $(shell command -v docker-compose 2>/dev/null),docker-compose,docker compose))

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

# ── WAN emulation ─────────────────────────────────────────────────────────────────
# All of this is inert unless WAN_ENABLED=1. When off, IMAGE_NAME_EFFECTIVE and
# DOCKER_TARGET resolve to exactly what the build used before WAN support existed
# (`final` is the last stage in the Dockerfile, so --target final is a no-op).
WAN                  := $(if $(filter 1,$(WAN_ENABLED)),1,)
IMAGE_NAME_EFFECTIVE := $(if $(WAN),$(IMAGE_NAME)-wan,$(IMAGE_NAME))
DOCKER_TARGET        := $(if $(WAN),final-wan,final)
NETEM_PROBE_IMAGE    := atlas-netem-probe

# Shell snippet expanding to the nodes that carry a shaping spec.
WAN_NODE_LIST = $$(seq 0 $$(($(N_REPLICAS)-1)) | sed 's/^/replica-/'; \
                  if [ "$(WAN_SHAPE_CLIENTS)" = "1" ]; then \
                      seq 0 $$(($(N_CLIENT_MACHINES)-1)) | sed 's/^/client-/'; fi)

.PHONY: local stop-local logs-local \
        remote-docker stop-remote-docker \
        remote-bare stop-remote-bare build-binary \
        gen-configs gen-ca-root regen-ca-root clean help \
        ensure-lockfile \
        wan-plan wan-check wan-show wan-apply

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
	@echo "  clean              Remove all generated files (configs, ca-root, logs)"
	@echo ""
	@echo "  wan-check          Verify this host's kernel can do netem (run this first)"
	@echo "  wan-plan           Compile the WAN profile and print the latency matrix"
	@echo "  wan-show           Show tc stats inside the running containers"
	@echo "  wan-apply          Re-apply the WAN profile to a running cluster"
	@echo ""
	@echo "Key variables (override on command line or in project bench.env):"
	@echo "  N_REPLICAS=$(N_REPLICAS)  N_CLIENTS=$(N_CLIENTS) (per machine)  N_CLIENT_MACHINES=$(N_CLIENT_MACHINES)"
	@echo "  Total logical clients = N_CLIENTS x N_CLIENT_MACHINES = $$(( $(N_CLIENTS) * $(N_CLIENT_MACHINES) ))"
	@echo "  CONCURRENT_RQS=$(CONCURRENT_RQS)  OPS_NUMBER=$(OPS_NUMBER)"
	@echo "  WAN_ENABLED=$(WAN_ENABLED)  WAN_PROFILE=$(WAN_PROFILE)  WAN_SUBNET=$(WAN_SUBNET)"
	@echo "  WAN_SHAPE_CLIENTS=$(WAN_SHAPE_CLIENTS)  WAN_TIMEOUT_SCALE=$(WAN_TIMEOUT_SCALE)"
	@echo ""
	@echo "Available WAN profiles: $$(ls $(GLOBAL_BENCH_DIR)/wan-profiles/*.yml 2>/dev/null | xargs -n1 basename 2>/dev/null | sed 's/\.yml$$//' | tr '\n' ' ')"
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

# ── Lockfile ───────────────────────────────────────────────────────────────────────
# The Docker build runs `cargo build --locked`, which errors instead of writing a
# missing Cargo.lock. Generate it here first (host cargo, has network access) so a
# fresh checkout builds without a manual `cargo generate-lockfile` step.
ensure-lockfile:
	@if [ ! -f $(RUST_SRC_ABS)/Cargo.lock ]; then \
	    echo "Cargo.lock missing in $(RUST_SRC_ABS), generating..."; \
	    (cd $(RUST_SRC_ABS) && cargo generate-lockfile); \
	fi

# ── Local Docker Compose ──────────────────────────────────────────────────────────
local: gen-configs ensure-lockfile
	@bash $(SCRIPTS)/gen-nodes-toml.sh local $(N_REPLICAS) $(N_CLIENT_MACHINES)
	@cp $(GENERATED)/nodes.toml $(GENERATED)/config-replicas/nodes.toml
	@cp $(GENERATED)/nodes.toml $(GENERATED)/config-clients/nodes.toml
ifneq ($(WAN),)
	@python3 $(SCRIPTS)/gen-wan.py
endif
	@bash $(SCRIPTS)/gen-local-compose.sh
	$(DOCKER) build -t $(IMAGE_NAME_EFFECTIVE) \
	    --target $(DOCKER_TARGET) \
	    --build-arg APP_NAME=$(BINARY_NAME) \
	    --build-arg APP_SOURCE_DIR=$(APP_SOURCE_DIR) \
	    -f $(DOCKERFILE_ABS) $(BUILD_CTX_ABS)
ifneq ($(WAN),)
	@want="$(WAN_SUBNET)"; \
	 have=$$($(DOCKER) network inspect atlas_network \
	         -f '{{range .IPAM.Config}}{{.Subnet}}{{end}}' 2>/dev/null || true); \
	 if [ "$$have" != "$$want" ]; then \
	     if [ -n "$$have" ]; then \
	         echo "Recreating atlas_network: subnet '$$have' -> '$$want'"; \
	     fi; \
	     $(DOCKER) network rm atlas_network 2>/dev/null || true; \
	     $(DOCKER) network create --subnet="$$want" atlas_network; \
	 fi
	@echo "WAN emulation ON — profile '$(WAN_PROFILE)', subnet $(WAN_SUBNET)."
	@echo "Inspect the topology with: make <project> wan-plan"
else
	$(DOCKER) network create atlas_network 2>/dev/null || true
endif
	@bash -c 'trap "$(COMPOSE) -f $(GENERATED)/local-compose.yml down; sleep 1; $(DOCKER) network rm atlas_network 2>/dev/null || true" EXIT; $(COMPOSE) -f $(GENERATED)/local-compose.yml up' || true

stop-local:
	$(COMPOSE) -f $(GENERATED)/local-compose.yml down
	$(DOCKER) network rm atlas_network 2>/dev/null || true

logs-local:
	$(COMPOSE) -f $(GENERATED)/local-compose.yml logs -f

# ── Remote Docker ─────────────────────────────────────────────────────────────────
remote-docker: gen-configs ensure-lockfile
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
	@echo "Compiling $(BINARY_NAME)$(if $(EXECUTOR_VARIANT), [executor variant: $(EXECUTOR_VARIANT)],)..."
	cd $(RUST_SRC_ABS) && RUSTFLAGS="-C target-cpu=native" cargo build --release \
	    $(if $(EXECUTOR_VARIANT),--no-default-features --features $(EXECUTOR_VARIANT),)
	@mkdir -p $(GENERATED)
	@cp $(RUST_SRC_ABS)/target/release/$(BINARY_NAME) $(GENERATED)/$(BINARY_NAME)
	@echo "Binary copied to $(GENERATED)/$(BINARY_NAME)"

# ── WAN emulation ─────────────────────────────────────────────────────────────────
# Shaping is compiled from bench/wan-profiles/$(WAN_PROFILE).yml into per-node tc
# scripts under generated/wan/, which the `final-wan` image applies at startup.

# Compile the profile and print the resulting latency matrix. Touches no containers,
# so this is the loop to use while authoring a profile.
wan-plan:
	@python3 $(SCRIPTS)/gen-wan.py
	@echo ""
	@cat $(GENERATED)/wan/matrix.txt

# Preflight: does this host's kernel actually have sch_netem? Everything else can be
# correct and shaping will still fail without it, so check before a long build.
wan-check:
	@mkdir -p $(GENERATED)/wan
	@printf 'FROM debian:bullseye-slim\nRUN apt-get update && apt-get install -y --no-install-recommends iproute2 && rm -rf /var/lib/apt/lists/*\n' \
	    > $(GENERATED)/wan/Dockerfile.probe
	@echo "Building netem probe image..."
	@$(DOCKER) build -t $(NETEM_PROBE_IMAGE) -f $(GENERATED)/wan/Dockerfile.probe $(GENERATED)/wan >/dev/null
	@echo "Probing sch_netem via $(DOCKER)..."
	@if $(DOCKER) run --rm --cap-add NET_ADMIN $(NETEM_PROBE_IMAGE) \
	      sh -c 'tc qdisc add dev lo root netem delay 1ms && tc qdisc show dev lo' 2>/dev/null \
	      | grep -q netem; then \
	    echo ""; \
	    echo "OK: netem is available. WAN emulation will work on this host."; \
	else \
	    echo ""; \
	    echo "FAIL: this kernel has no sch_netem — WAN emulation cannot work here."; \
	    echo ""; \
	    echo "netem is the only qdisc that can add delay, so there is no workaround."; \
	    echo "HTB alone (which may well be present) gives bandwidth caps but no latency."; \
	    echo ""; \
	    echo "If you are on macOS using podman, the podman machine VM runs Fedora"; \
	    echo "CoreOS, whose base image omits sch_netem. Add it once with:"; \
	    echo ""; \
	    echo "    podman machine ssh 'sudo rpm-ostree install kernel-modules-extra'"; \
	    echo "    podman machine stop && podman machine start"; \
	    echo "    make <project> wan-check      # confirm"; \
	    echo ""; \
	    echo "The layered package survives VM reboots but not 'podman machine rm'."; \
	    echo "Roll back with: podman machine ssh 'sudo rpm-ostree rollback'"; \
	    echo ""; \
	    echo "On Docker Desktop, check that the LinuxKit kernel exposes sch_netem;"; \
	    echo "on a Linux host, install the distro's kernel-modules-extra equivalent."; \
	    exit 1; \
	fi

# Show what is actually installed in the running containers. Reports byte/packet
# counters as well as configuration: a climbing "dropped" on a class means that
# class's netem limit is too small for the bandwidth-delay product, which shows up
# as phantom packet loss in results.
wan-show:
	@for n in $(WAN_NODE_LIST); do \
	    echo "═════ $$n ═════"; \
	    $(DOCKER) exec $$n tc -s qdisc show dev $(WAN_IFACE) 2>/dev/null \
	        || echo "  (not running, or not a WAN-enabled container)"; \
	    $(DOCKER) exec $$n tc -s class show dev $(WAN_IFACE) 2>/dev/null || true; \
	done

# Recompile the profile and push it into an already-running cluster. The generated
# scripts delete the root qdisc first, so this is idempotent and can be re-run to
# switch profiles mid-experiment.
wan-apply:
	@if [ "$(WAN_ENABLED)" != "1" ]; then \
	    echo "ERROR: wan-apply needs WAN_ENABLED=1 (the containers must be running the"; \
	    echo "       final-wan image with /wan mounted and CAP_NET_ADMIN)."; \
	    echo "       Re-run with: make <project> wan-apply WAN_ENABLED=1"; \
	    exit 1; \
	fi
	@python3 $(SCRIPTS)/gen-wan.py
	@for n in $(WAN_NODE_LIST); do \
	    printf '%s: ' "$$n"; \
	    $(DOCKER) exec $$n sh /wan/$$n.sh || echo "  FAILED on $$n"; \
	done
	@echo ""
	@cat $(GENERATED)/wan/matrix.txt

# ── Cleanup ───────────────────────────────────────────────────────────────────────
clean:
	rm -rf $(GENERATED)
	@echo "Cleaned $(GENERATED)"
