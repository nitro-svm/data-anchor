set unstable := true

# The budget for the arbtest program in milliseconds

export ARBTEST_BUDGET_MS := "10000"
anchor-rust := "1.86.0"
solana-cmd := '''
    solana-test-validator \
        --reset \
        --clone-feature-set -u d \
        --ledger target/test-ledger \
        --limit-ledger-size 1000000 \
        --bpf-program anchorE4RzhiFx3TEFep6yRNK9igZBzMVWziqjbGHp2 programs/target/deploy/data_anchor_blober.so \
    '''

[group('lint')]
check-udeps:
    cargo +nightly udeps --workspace --all-targets --all-features

[group('lint')]
[private]
fmt-justfile:
    just --fmt --check

# Run lint and formatting checks for the programs directory
[group('lint')]
[working-directory('programs')]
lint-programs:
    cargo +nightly fmt -- --check
    cargo +{{ anchor-rust }} clippy --all-targets --all-features
    zepter run check

# Run lint and formatting checks for the entire project
[group('lint')]
lint: lint-programs fmt-justfile build-prover
    cargo +nightly fmt -- --check
    cargo clippy --all-targets --all-features
    zepter

[group('lint')]
[private]
fmt-justfile-fix:
    just --fmt

# Fix lint and formatting issues in the programs directory
[group('lint')]
[working-directory('programs')]
lint-programs-fix:
    cargo +nightly fmt
    cargo +{{ anchor-rust }} clippy --fix --allow-dirty --allow-staged --all-targets --all-features
    zepter

# Fix lint and formatting issues in the entire project
[group('lint')]
lint-fix: lint-programs-fix fmt-justfile-fix build-prover
    cargo +nightly fmt
    cargo clippy --fix --allow-dirty --allow-staged --all-targets --all-features
    zepter

# Run tests for the programs directory
[group('test')]
[working-directory('programs')]
test-programs: build-programs
    cargo +{{ anchor-rust }} nextest run --workspace

# Run compute budget tests for transaction fees
[group('test')]
test-compute-unit-limit limit=ARBTEST_BUDGET_MS: run-solana-test-validator && stop-solana-test-validator
    @sleep 10
    ARBTEST_BUDGET_MS={{ limit }} cargo nextest run --workspace -E 'test(compute_unit_limit)' -- --ignored

# Run tests for the crates in the workspace
[group('test')]
test:
    cargo nextest run --workspace

# Run tests for the entire project
[group('test')]
test-all: test-programs test

# Run pre-push checks
[group('dev')]
pre-push: lint-fix test-all

# Build the programs
[group('build')]
[working-directory('programs')]
build-programs:
    anchor build --no-idl

# Build the prover script
[group('build')]
build-prover:
    cargo build --release -p data-anchor-prover

# Build the entire project
[group('build')]
build: build-programs
    cargo build --release

# Sync blober program keys
[group('program-utils')]
[working-directory('programs')]
sync-keys network:
    anchor keys sync --provider.cluster {{ network }}

# Deploy the blober program
[confirm('Are you sure you want to deploy the blober program [y/n]?')]
[group('program-utils')]
[working-directory('programs')]
deploy network:
    anchor keys sync --provider.cluster {{ network }}
    anchor build --no-idl
    anchor deploy --provider.cluster {{ network }}

[group('program-utils')]
init-blober namespace:
    cargo run -p data-anchor -- -i ws://localhost:9696 -n {{ namespace }} br i

[confirm('This will run benchmarks against a deployed program and will take a while. Are you sure you want to continue [y/n]?')]
[group('program-utils')]
run-benchmark:
    @echo "Running benchmark with default config"
    cargo run --release -p data-anchor -- -n bench m a --data-path ./target/data

# Clean the programs directory
[group('clean')]
[working-directory('programs')]
clean-programs:
    cargo clean

# Clean the entire project
[group('clean')]
clean: clean-programs
    cargo clean

[group('dev')]
[private]
ensure-logs:
    @mkdir -p target/logs

[private]
start-process cmd name: ensure-logs
    #!/usr/bin/env bash
    set -euo pipefail
    {{ cmd }} 1>./target/logs/{{ name }}.log 2>&1 &
    echo $! > ./target/{{ name }}.pid
    echo "{{ name }} started with PID $(cat ./target/{{ name }}.pid)"

[private]
stop-process name:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -f ./target/{{ name }}.pid ]; then
        kill -9 $(cat ./target/{{ name }}.pid) 
        rm ./target/{{ name }}.pid
        echo "{{ name }} stopped."
    else
        echo "No {{ name }} PID file found. Is {{ name }} running?"
    fi

# Run the solana-test-validator with the blober program
[group('solana')]
run-solana-test-validator: build-programs (stop-process 'solana-test-validator')
    @sleep 2
    @just start-process "{{ solana-cmd }}" solana-test-validator

# Stop the solana-test-validator
[group('solana')]
stop-solana-test-validator: (stop-process 'solana-test-validator')

# Run full workflow tests on a local network - the local network must be running
[group('test')]
test-with-local: run-solana-test-validator && stop-solana-test-validator
    @sleep 10
    cargo nextest run --workspace -E 'test(full_workflow_localnet)' -- --ignored

# Run local e2e process and calculate on-chain cost
[confirm('This will run all the indexer components and run CLI commands against it. Are you sure you want to continue [y/n]?')]
[group('test')]
calculate-on-chain-cost:
    ./scripts/calculate-on-chain-cost.sh

# Run prover script for different elfs
[confirm('This will run the prover script all ELFs (might take a long time). Are you sure you want to continue [y/n]?')]
[group('test')]
run-prover prove='' verify='':
    cargo run --release -p data-anchor-prover-script {{ prove && '-p' + verify && ' -v' }} 2>&1

# Run the client example
[group('dev')]
[working-directory('examples/examples')]
run-client-example api-token='':
    cargo run --example client -- {{ api-token && '--indexer-api-token ' + api-token }}

# Run the CLI example
[group('dev')]
[working-directory('examples/examples/cli')]
run-cli-example:
    ./cli.sh
