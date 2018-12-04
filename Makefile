# holochain-rust Makefile
# currently only supports 'debug' builds

# run `make` to build all the libraries and binaries
# run `make test` to execute all the tests
# run `make clean` to clean up the build environment
# run `make test_holochain` to test holochain builds
# run `make test_cmd` to test the command line tool builds

all: lint build_holochain build_cmd

SHELL = /bin/bash
CORE_RUST_VERSION ?= nightly-2018-11-28
TOOLS_RUST_VERSION ?= nightly-2018-11-28
CARGO = RUSTFLAGS="-Z external-macro-backtrace -D warnings" RUST_BACKTRACE=1 rustup run $(CORE_RUST_VERSION) cargo $(CARGO_ARGS)
CARGO_TOOLS = RUSTFLAGS="-Z external-macro-backtrace -D warnings" RUST_BACKTRACE=1 rustup run $(TOOLS_RUST_VERSION) cargo $(CARGO_ARGS)
CARGO_TARPULIN_INSTALL = RUSTFLAGS="--cfg procmacro2_semver_exempt -D warnings" RUST_BACKTRACE=1 cargo $(CARGO_ARGS) +$(CORE_RUST_VERSION)

# list all the "C" binding tests that have been written
C_BINDING_DIRS = $(sort $(dir $(wildcard c_binding_tests/*/)))

# list all the "C" binding test executables that should be produced
C_BINDING_TESTS = $(foreach dir,$(C_BINDING_DIRS),target/debug/c_binding_tests/$(shell basename $(dir))/test_executable)

# list all the extraneous files that will be generated when running tests
C_BINDING_CLEAN = $(foreach dir,$(C_BINDING_DIRS),$(dir)Makefile $(dir).qmake.stash)

# build artifact / dependency checking is handled by our sub-tools
# we can just try to build everything every time, it should be efficient
.PHONY: lint \
	c_binding_tests ${C_BINDING_DIRS} \
	test ${C_BINDING_TESTS} \
	test_holochain \
	clean ${C_BINDING_CLEAN}

# apply formatting / style guidelines
lint: fmt_check clippy

# Check if Rust version is correct, and prompts to offer to change to the correct version.  Requires
# RUST_VERSION to be set, as appropriate for whatever target is being installed (defaults to
# CORE_RUST_VERSION; see install_rustup..., below).  We'll also export PATH to default location of
# Rust installation for use here in the Makefile, in case this is the first time rustup has been
# installed/run, and we don't have a rustup-modified .profile loaded yet.  If not connected to a
# terminal (stdin is a tty), or running under a Continuous Integration test (CI), defaults to
# automatically installing and changing the default Rust version (under the assumption that the
# invoker of the Makefile target knows what they want, under headless automated procedures like
# CI). Otherwise, entering "no<return>" rejects installing/changing the Rust version (and we assume
# you know what you're doing, eg. testing some new Rust toolchain version that you've installed)
export PATH := $(HOME)/.cargo/bin:$(PATH)
RUST_VERSION = $(CORE_RUST_VERSION)
.PHONY: version_rustup
version_rustup:
	@if which rustup; then \
	    echo "\033[0;93m## Current Rust version installed (need: '$(RUST_VERSION)'): ##\033[0m"; \
	    if ! rustup show 2>/dev/null | grep -qe "$(RUST_VERSION).*(default)"; then \
	        rustup show; \
		echo "\033[0;93m## Change current Rust version to '$(RUST_VERSION)' ##\033[0m"; \
	        [ -t 1 ] && [[ "$(CI)" == "" ]] && read -p "Continue? (Y/n) " yes; \
	        if [[ "$${yes:0:1}" != "n" ]] && [[ "$${yes:0:1}" != "N" ]]; then \
	            echo "\033[0;93m## Selecting Rust version '$(RUST_VERSION)'... ##\033[0m"; \
	            rustup default $(RUST_VERSION); \
	        fi; \
	    fi; \
	fi

# Actual installation of Rust $(RUST_VERSION) via curl
.PHONY: curl_rustup
curl_rustup:
	@if ! which rustup ; then \
	    echo "\033[0;93m## Installing Rust $(RUST_VERSION)... ##\033[0m"; \
	    curl https://sh.rustup.rs -sSf | sh -s -- --default-toolchain $(RUST_VERSION) -y; \
	fi

# idempotent install rustup with the default toolchain set for Holochain core
# best for green fields Rust installation
.PHONY: install_rustup
install_rustup:		RUST_VERSION = $(CORE_RUST_VERSION)
install_rustup: version_rustup curl_rustup

# idempotent install rustup with the default toolchain set for tooling
# best for CI based on tools only.
.PHONY: install_rustup_tools
install_rustup_tools:	RUST_VERSION = $(TOOLS_RUST_VERSION)
install_rustup_tools: version_rustup curl_rustup


# idempotent installation of libzmq system library
# note, this is complicated by our use of travis-ci ubuntu trusty
# we need to install a newer version than is otherwise available
.PHONY: install_system_libzmq
install_system_libzmq:
	@if ! (pkg-config libzmq --libs) ; then \
		if ! which apt-get ; then \
			if which brew ; then \
				echo "\033[0;93m## Attempting to install zmq using homebrew ##\033[0m"; \
				brew install zmq; \
			else \
				echo "\033[0;93m## libzmq couldn't be installed, build probably won't work ##\033[0m"; \
			fi; \
		else \
			if [ "x${TRAVIS}" = "x" ]; then \
				echo "\033[0;93m## Attempting to install libzmq3-dev with apt-get ##\033[0m"; \
				sudo apt-get install -y libzmq3-dev; \
			else \
				echo "\033[0;93m## Attempting to install libzmq3-dev on UBUNTU TRUSTY ##\033[0m"; \
				echo "deb http://download.opensuse.org/repositories/network:/messaging:/zeromq:/release-stable/xUbuntu_14.04/ ./" >> /etc/apt/sources.list; \
				wget https://download.opensuse.org/repositories/network:/messaging:/zeromq:/release-stable/xUbuntu_14.04/Release.key -O- | sudo apt-key add; \
				sudo apt-get update -qq; \
				sudo apt-get install libzmq3-dev; \
			fi; \
		fi; \
	fi; \

# idempotent install of any required system libraries
.PHONY: install_system_libs
install_system_libs: install_system_libzmq

# idempotent installation of core toolchain.  Changes default toolchain to CORE_RUST_VERSION.
.PHONY: core_toolchain
core_toolchain: RUST_VERSION=$(CORE_RUST_VERSION)
core_toolchain: version_rustup install_rustup install_system_libs

# idempotent installation of tools toolchain.  Changes default toolchain to TOOLS_RUST_VERSION.
.PHONY: tools_toolchain
tools_toolchain: RUST_VERSION=$(TOOLS_RUST_VERSION)
tools_toolchain: version_rustup install_rustup_tools install_system_libs

# idempotent addition of wasm target in current (default: CORE_RUST_VERSION) toolchain
.PHONY: ensure_wasm_target
ensure_wasm_target: core_toolchain
	rustup target add wasm32-unknown-unknown

# idempotent installation of development tooling; RUST_VERSION defaults to TOOLS_RUST_VERSION
.PHONY: install_rust_tools
install_rust_tools: tools_toolchain
	# rust format
	if ! rustup component list --toolchain $(RUST_VERSION) | grep 'rustfmt-preview.*(installed)'; then \
		rustup component add --toolchain $(RUST_VERSION) rustfmt-preview; \
	fi
	# clippy
	if ! rustup component list --toolchain $(RUST_VERSION) | grep 'clippy-preview.*(installed)'; then \
		rustup component add --toolchain $(RUST_VERSION) clippy-preview; \
	fi

# idempotent installation of code coverage CI/testing tools
.PHONY: install_ci
install_ci: core_toolchain
	# tarpaulin (code coverage)
	if ! $(CARGO) install --list | grep 'cargo-tarpaulin'; then \
		 $(CARGO_TARPULIN_INSTALL) install cargo-tarpaulin --force; \
	fi

.PHONY: install_mdbook
install_mdbook: tools_toolchain
	if ! $(CARGO_TOOLS) install --list | grep 'mdbook'; then \
	    $(CARGO_TOOLS) install mdbook --vers "^0.2.2"; \
	fi

# list all our found "C" binding tests
c_binding_tests: ${C_BINDING_DIRS}

# build all our found "C" binding tests
${C_BINDING_DIRS}:
	qmake -o $@Makefile $@qmake.pro
	cd $@; $(MAKE)

# execute all tests: holochain, command-line tools, app spec, nodejs container, and "C" bindings
test: test_holochain test_cmd test_app_spec c_binding_tests ${C_BINDING_TESTS}

test_holochain: build_holochain
	RUSTFLAGS="-D warnings" $(CARGO) test --all --exclude hc

test_cmd: build_cmd
	cd cmd && RUSTFLAGS="-D warnings" $(CARGO) test

test_app_spec: RUST_VERSION=$(CORE_RUST_VERSION)
test_app_spec: version_rustup ensure_wasm_target install_cmd build_nodejs_container
	cd app_spec && ./build_and_test.sh

build_nodejs_container: RUST_VERSION=$(CORE_RUST_VERSION)
build_nodejs_container: version_rustup core_toolchain
	./scripts/build_nodejs_container.sh

c_build: core_toolchain
	cd dna_c_binding && $(CARGO) build

test_c_ci: c_build c_binding_tests ${C_BINDING_TESTS}

.PHONY: wasm_build
wasm_build: ensure_wasm_target
	cd core/src/nucleus/actions/wasm-test && $(CARGO) build --release --target wasm32-unknown-unknown
	cd container_api/wasm-test && $(CARGO) build --release --target wasm32-unknown-unknown
	cd hdk-rust/wasm-test && $(CARGO) build --release --target wasm32-unknown-unknown
	cd wasm_utils/wasm-test/integration-test && $(CARGO) build --release --target wasm32-unknown-unknown

.PHONY: build_holochain
build_holochain: core_toolchain wasm_build
	$(CARGO) build --all --exclude hc

.PHONY: build_cmd
build_cmd: core_toolchain ensure_wasm_target
	$(CARGO) build -p hc

.PHONY: install_cmd
install_cmd: build_cmd
	cd cmd && $(CARGO) install -f --path .

.PHONY: code_coverage
code_coverage: core_toolchain wasm_build install_ci
	$(CARGO) tarpaulin --ignore-tests --timeout 600 --all --out Xml --skip-clean -v -e holochain_core_api_c_binding -e hdk -e hc -e holochain_core_types_derive

.PHONY: code_coverage_crate
code_coverage_crate: core_toolchain wasm_build install_ci
	$(CARGO) tarpaulin --ignore-tests --timeout 600 --skip-clean -v -p $(CRATE)

fmt_check: install_rust_tools
	$(CARGO_TOOLS) fmt -- --check

clippy: install_rust_tools
	$(CARGO_TOOLS) clippy -- -A needless_return --A useless_attribute

fmt: install_rust_tools
	$(CARGO_TOOLS) fmt

# execute all the found "C" binding tests
${C_BINDING_TESTS}:
	$@

# clean up the target directory and all extraneous "C" binding test files
clean: ${C_BINDING_CLEAN}
	-@$(RM) -rf target
	-@$(RM) -rf wasm_utils/wasm-test/integration-test/target

# clean up the extraneous "C" binding test files
${C_BINDING_CLEAN}:
	-@$(RM) $@
