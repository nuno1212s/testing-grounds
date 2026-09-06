# testing-grounds/Makefile
#
# Top-level entry point for the shared bench system.
# Usage: make <project> <target> [VAR=value ...]
#
# Examples:
#   make microbenchmarks-async local
#   make app-scaling-tests remote-docker
#   make microbenchmark-hotstuff local N_REPLICAS=3

PROJECTS := \
    microbenchmarks-async \
    app-scaling-tests \
    preemptive_execution \
    microbenchmarks \
    microbenchmark-hotstuff \
    microbenchmark-chainedhotstuff \
    crud_perf \
    correctness-testing

MODES := local stop-local logs-local \
         remote-docker stop-remote-docker \
         remote-bare stop-remote-bare \
         build-binary gen-configs \
         wan-check wan-plan wan-show wan-apply \
         clean clean-docker clean-cargo distclean help

# Cleanup targets that act on the shared/global bench dir and can run without a
# project named (naming one additionally purges that project's image / target/).
PROJECT_FREE := clean clean-docker distclean help

# Mapping: project name → bench dir (relative to this Makefile's directory)
bench_dir_microbenchmarks-async          := microbenchmarks-async/bench
bench_dir_app-scaling-tests              := app-scaling-tests/bench
bench_dir_preemptive_execution           := preemptive_execution/bench
bench_dir_microbenchmarks                := microbenchmarks/bench
bench_dir_microbenchmark-hotstuff        := hot_stuff/microbenchmark-hotstuff/bench
bench_dir_microbenchmark-chainedhotstuff := hot_stuff/microbenchmark-chainedhotstuff/bench
bench_dir_crud_perf                      := crud_perf/bench
bench_dir_correctness-testing            := correctness-testing/bench

CURRENT_PROJECT := $(firstword $(filter $(PROJECTS), $(MAKECMDGOALS)))
GLOBAL_BENCH_DIR := $(CURDIR)/bench
SHARED_MAKEFILE  := $(GLOBAL_BENCH_DIR)/Makefile

# Project name targets are no-ops; mode targets do the actual work
.PHONY: $(PROJECTS) $(MODES)

$(PROJECTS): ;

$(MODES):
	@if [ -n "$(CURRENT_PROJECT)" ]; then \
	    PROJECT_BENCH="$(CURDIR)/$(bench_dir_$(CURRENT_PROJECT))"; \
	    if [ ! -d "$$PROJECT_BENCH" ]; then \
	        echo "ERROR: bench dir not found: $$PROJECT_BENCH"; \
	        echo "Create $(bench_dir_$(CURRENT_PROJECT))/bench.env with project identity variables."; \
	        exit 1; \
	    fi; \
	    $(MAKE) -f $(SHARED_MAKEFILE) $@ \
	        GLOBAL_BENCH_DIR=$(GLOBAL_BENCH_DIR) \
	        PROJECT_BENCH_DIR=$$PROJECT_BENCH; \
	elif [ -n "$(filter $@,$(PROJECT_FREE))" ]; then \
	    $(MAKE) -f $(SHARED_MAKEFILE) $@ GLOBAL_BENCH_DIR=$(GLOBAL_BENCH_DIR); \
	else \
	    echo "Usage: make <project> <target> [VAR=value ...]"; \
	    echo ""; \
	    echo "Projects:"; \
	    for p in $(PROJECTS); do echo "  $$p"; done; \
	    echo ""; \
	    echo "Targets: $(MODES)"; \
	    exit 1; \
	fi
