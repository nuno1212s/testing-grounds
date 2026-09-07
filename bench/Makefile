# Shared bench Makefile — called by testing-grounds/Makefile as:
#   make -f testing-grounds/bench/Makefile <target> \
#        GLOBAL_BENCH_DIR=/abs/path/testing-grounds/bench \
#        PROJECT_BENCH_DIR=/abs/path/<project>/bench

# Targets that act on the shared bench dir only and so do not need a project named:
# cleanup, help, and the metrics stack (whose InfluxDB target comes from the global
# config-base, not from any one project).
PROJECT_FREE_TARGETS := clean help \
                        metrics stop-metrics logs-metrics clean-metrics \
                        grafana stop-grafana logs-grafana clean-grafana

ifndef GLOBAL_BENCH_DIR
$(error GLOBAL_BENCH_DIR is not set. Invoke via testing-grounds/Makefile: make <project> <target>)
endif
ifndef PROJECT_BENCH_DIR
ifeq ($(filter $(PROJECT_FREE_TARGETS),$(MAKECMDGOALS)),)
$(error PROJECT_BENCH_DIR is not set. Invoke via testing-grounds/Makefile: make <project> <target>)
endif
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

# ── Metrics stack ─────────────────────────────────────────────────────────────────
# InfluxDB + Grafana, a Compose project of its own (bench/grafana/) joined to the same
# atlas_network as the benchmark stack. Kept separate because it has to outlive the
# run: the database is read after the run ends, so it cannot be torn down with it.
#
# The influxdb service sits behind the `local-influx` Compose profile, which is how
# LOCAL_INFLUXDB=0 (external instance) starts Grafana alone. On `down` the profile is
# passed unconditionally, so a stack started with the profile is still fully removed
# after LOCAL_INFLUXDB has been flipped back to 0.
GRAFANA_DIR     := $(GLOBAL_BENCH_DIR)/grafana
GRAFANA_COMPOSE := $(GRAFANA_DIR)/docker-compose.yml
GRAFANA_PROJECT := atlas-grafana
METRICS_DOWN    := $(COMPOSE) --profile local-influx -p $(GRAFANA_PROJECT) -f $(GRAFANA_COMPOSE)
METRICS_AUTOSTART ?= 1
# Only selects which warnings metrics-up.sh prints; set per run target below.
METRICS_MODE      ?= explicit

# Shell snippet expanding to the nodes that carry a shaping spec.
WAN_NODE_LIST = $$(seq 0 $$(($(N_REPLICAS)-1)) | sed 's/^/replica-/'; \
                  if [ "$(WAN_SHAPE_CLIENTS)" = "1" ]; then \
                      seq 0 $$(($(N_CLIENT_MACHINES)-1)) | sed 's/^/client-/'; fi)

.PHONY: local stop-local logs-local \
        remote-docker stop-remote-docker \
        remote-bare stop-remote-bare build-binary \
        gen-configs gen-ca-root regen-ca-root clean help \
        ensure-lockfile ensure-network \
        metrics stop-metrics logs-metrics clean-metrics \
        grafana stop-grafana logs-grafana clean-grafana ensure-metrics \
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
	@echo "  metrics            Start InfluxDB + Grafana on atlas_network and wait for the DB"
	@echo "  stop-metrics       Stop the metrics stack (keeps its data volumes)"
	@echo "  logs-metrics       Follow InfluxDB + Grafana logs"
	@echo "  clean-metrics      Stop the metrics stack and delete its data volumes"
	@echo "                     (grafana / stop-grafana / logs-grafana / clean-grafana are aliases)"
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
	@echo "  LOCAL_INFLUXDB=$(LOCAL_INFLUXDB)  METRICS_AUTOSTART=$(METRICS_AUTOSTART)"
	@echo "  GRAFANA_PORT=$(GRAFANA_PORT)  GRAFANA_BIND=$(GRAFANA_BIND)"
	@echo "  INFLUXDB_PORT=$(INFLUXDB_PORT)  INFLUXDB_BIND=$(INFLUXDB_BIND)  INFLUXDB_RETENTION=$(INFLUXDB_RETENTION)"
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

# ── Shared network ────────────────────────────────────────────────────────────────
# atlas_network is the one thing the benchmark stack and the Grafana stack have in
# common: both declare it `external: true`, so it must exist before either comes up
# and neither may own its lifetime. This target is the single place that creates it.
#
# Teardown is deliberately best-effort everywhere (`network rm ... || true`): when the
# metrics stack is attached, removing the network fails, the network survives the run,
# and InfluxDB and Grafana keep working — which is the point.
#
# In WAN mode the network additionally has to carry WAN_SUBNET, because the tc filters
# match on destination IP and so the addresses must be static.
ensure-network:
ifneq ($(WAN),)
	@want="$(WAN_SUBNET)"; \
	 have=$$($(DOCKER) network inspect atlas_network \
	         -f '{{range .IPAM.Config}}{{.Subnet}}{{end}}' 2>/dev/null || true); \
	 if [ "$$have" != "$$want" ]; then \
	     if [ -n "$$have" ]; then \
	         echo "Recreating atlas_network: subnet '$$have' -> '$$want'"; \
	     fi; \
	     $(DOCKER) network rm atlas_network 2>/dev/null || true; \
	     $(DOCKER) network create --subnet="$$want" atlas_network || { \
	         echo ""; \
	         echo "ERROR: could not (re)create atlas_network with subnet $$want."; \
	         echo "       Most likely something is still attached to the old network —"; \
	         echo "       the metrics stack is the usual culprit, since it outlives a run:"; \
	         echo ""; \
	         echo "           make stop-metrics && make <project> local WAN_ENABLED=1"; \
	         echo ""; \
	         echo "       Check with: $(DOCKER) network inspect atlas_network"; \
	         exit 1; \
	     }; \
	 fi
else
	@$(DOCKER) network create atlas_network 2>/dev/null || true
endif

# ── Local Docker Compose ──────────────────────────────────────────────────────────
# ensure-metrics comes before anything that starts a node: see its comment below.
local: METRICS_MODE := local
local: gen-configs ensure-lockfile ensure-network ensure-metrics
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
	@echo "WAN emulation ON — profile '$(WAN_PROFILE)', subnet $(WAN_SUBNET)."
	@echo "Inspect the topology with: make <project> wan-plan"
endif
	@bash -c 'trap "$(COMPOSE) -f $(GENERATED)/local-compose.yml down; sleep 1; $(DOCKER) network rm atlas_network 2>/dev/null || true" EXIT; $(COMPOSE) -f $(GENERATED)/local-compose.yml up' || true

stop-local:
	$(COMPOSE) -f $(GENERATED)/local-compose.yml down
	$(DOCKER) network rm atlas_network 2>/dev/null || true

logs-local:
	$(COMPOSE) -f $(GENERATED)/local-compose.yml logs -f

# ── Remote Docker ─────────────────────────────────────────────────────────────────
remote-docker: METRICS_MODE := remote
remote-docker: gen-configs ensure-lockfile ensure-metrics
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
remote-bare: METRICS_MODE := remote
remote-bare: gen-configs build-binary ensure-metrics
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

# ── Metrics stack ─────────────────────────────────────────────────────────────────
# InfluxDB (where every node writes) and Grafana (where you read it), as their own
# Compose project on the shared atlas_network:
#
#   * with LOCAL_INFLUXDB=1 the stack runs the database, replicas and clients reach
#     it as http://influxdb:8086 over atlas_network, and Grafana reads the same
#     container — no host port needs to be published for any of that;
#   * with LOCAL_INFLUXDB=0 the influxdb service is filtered out by its Compose
#     profile and everything talks to the external instance from
#     config-base/common/influx_db.toml.
#
# gen-grafana.sh resolves which of the two applies and writes the server's env, the
# datasource and the credentials from one file, so the writers, the database and
# Grafana cannot end up on different databases.
#
# No project name is required: `make metrics` from testing-grounds/ is enough.
metrics: ensure-network
	@bash $(SCRIPTS)/metrics-up.sh explicit
	@echo "  Dashboards: folder 'Atlas'. Anonymous access is $(GRAFANA_ANONYMOUS); admin login is $(GRAFANA_ADMIN_USER)."

# Hook for the run targets. Same script, so the automatic path and `make metrics`
# cannot diverge; METRICS_AUTOSTART=0 opts out. `up -d` on a running stack is a
# no-op and the wait returns immediately, so this costs ~nothing once it is up.
#
# It is a hard prerequisite rather than a best-effort nicety on purpose: a node whose
# InfluxDB is unreachable aborts (os_mon's .expect + panic = "abort" in the release
# profile), so a run started before the database is up does not produce degraded
# results — it produces none.
ensure-metrics: ensure-network
ifeq ($(METRICS_AUTOSTART),1)
	@bash $(SCRIPTS)/metrics-up.sh $(METRICS_MODE)
else
	@echo "METRICS_AUTOSTART=0 — not starting the metrics stack."
	@echo "  Nodes still write to whatever generated/config-*/influx_db.toml names, and"
	@echo "  abort on the first failed write. Make sure that instance is up."
endif

stop-metrics:
	$(METRICS_DOWN) down
	@$(DOCKER) network rm atlas_network 2>/dev/null || true

logs-metrics:
	$(METRICS_DOWN) logs -f

# Also drops both data volumes: Grafana's own state (UI edits, users, starred
# dashboards) and, more to the point, every measurement InfluxDB has stored. The
# provisioned dashboards come straight back from bench/grafana/dashboards/; the
# measurements do not come back at all.
clean-metrics:
	$(METRICS_DOWN) down -v
	@rm -rf $(GENERATED)/grafana
	@$(DOCKER) network rm atlas_network 2>/dev/null || true
	@echo "Metrics stack stopped; data volumes and $(GENERATED)/grafana removed."

# Aliases kept from when the stack was Grafana alone.
grafana: metrics
stop-grafana: stop-metrics
logs-grafana: logs-metrics
clean-grafana: clean-metrics

# ── Cleanup ───────────────────────────────────────────────────────────────────────
clean:
	rm -rf $(GENERATED)
	@echo "Cleaned $(GENERATED)"
