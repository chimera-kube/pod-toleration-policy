HYPERFINE := $(shell command -v hyperfine 2> /dev/null)

.PHONY: build
build:
	cargo build --target=wasm32-wasi --release

.PHONY: registry
registry:
	@sh -c 'docker inspect registry &> /dev/null || docker run -d -p 5000:5000 --name registry registry:2'

.PHONY: publish
publish: build registry
	wasm-to-oci push target/wasm32-wasi/release/pod-toleration-policy.wasm localhost:5000/admission-wasm/pod-toleration-policy:v1

.PHONY: test
test:
	cargo test

.PHONY: clean
clean:
	cargo clean

.PHONY: bench
bench: build
ifndef HYPERFINE
	cargo install hyperfine
endif
	@printf "\nAccepting policy\n"
	hyperfine --warmup 10 "cat test_data/req_pod_with_toleration.json | wasmtime run --env TOLERATION_EFFECT="NoSchedule" --env TOLERATION_KEY="example-key" --env TOLERATION_OPERATOR="Exists" --env ALLOWED_GROUPS="system:authenticated" target/wasm32-wasi/release/pod-toleration-policy.wasm"

	@printf "\nRejecting policy\n"
	hyperfine --warmup 10 "cat test_data/req_pod_with_toleration.json | wasmtime run --env TOLERATION_EFFECT="NoSchedule" --env TOLERATION_KEY="example-key" --env TOLERATION_OPERATOR="Exists" --env ALLOWED_GROUPS="administrators" target/wasm32-wasi/release/pod-toleration-policy.wasm"

	@printf "\nOperation not relevant\n"
	hyperfine --warmup 10 "cat test_data/req_delete.json | wasmtime run --env TOLERATION_EFFECT="NoSchedule" --env TOLERATION_KEY="example-key" --env TOLERATION_OPERATOR="Exists" --env ALLOWED_GROUPS="administrators" target/wasm32-wasi/release/pod-toleration-policy.wasm"
