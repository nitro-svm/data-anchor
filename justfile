set unstable := true

# The Pubkey of the payer account for image building

export PAYER_PUBKEY := "2MCVmcuUcREwQKDS3HazuYctkkbZV3XRMspM5eLWRZUV"

# The budget for the arbtest program in milliseconds

export ARBTEST_BUDGET_MS := "10000"

[group('lint')]
[private]
fmt-justfile:
    just --fmt --check

# Run formatting checks for the infrastructure directory
[group('lint')]
[working-directory('infrastructure')]
fmt-tofu:
    tofu fmt -check

# Run lint and formatting checks for the programs directory
[group('lint')]
[working-directory('programs')]
lint-programs:
    cargo +nightly fmt -- --check
    cargo clippy --all-targets --all-features
    zepter run check

# Run lint and formatting checks for the entire project
[group('lint')]
lint: lint-programs fmt-justfile fmt-tofu build-prover
    cargo +nightly fmt -- --check
    cargo clippy --all-targets --all-features
    zepter

[group('lint')]
[private]
fmt-justfile-fix:
    just --fmt

# Fix formatting issues in the infrastructure directory
[group('lint')]
[working-directory('infrastructure')]
fmt-tofu-fix:
    tofu fmt

# Fix lint and formatting issues in the programs directory
[group('lint')]
[working-directory('programs')]
lint-programs-fix:
    cargo +nightly fmt
    cargo clippy --fix --allow-dirty --allow-staged --all-targets --all-features
    zepter

# Fix lint and formatting issues in the entire project
[group('lint')]
lint-fix: lint-programs-fix fmt-justfile-fix fmt-tofu-fix build-prover
    cargo +nightly fmt
    cargo clippy --fix --allow-dirty --allow-staged --all-targets --all-features
    zepter

# Run tests for the programs directory
[group('test')]
[working-directory('programs')]
test-programs: build-programs
    cargo nextest run --workspace

# Run compute budget tests for transaction fees
[group('test')]
test-compute-unit-limit:
    cargo nextest run --workspace -E 'test(compute_unit_limit)' -- --ignored

# Run tests for the crates in the workspace
[group('test')]
test:
    cargo nextest run --workspace

# Run tests for the entire project
[group('test')]
test-all: test-programs test

# Run full workflow tests on a local network - the local network must be running
[group('test')]
test-with-local:
    cargo nextest run --workspace -E 'test(full_workflow_localnet)' -- --ignored

[confirm('This will run the indexer tests and requires a local database to be running. Are you sure you want to continue [y/n]?')]
[group('test')]
test-indexer:
    cargo nextest run --workspace -j1 -E 'test(indexer)' -- --ignored

# Run indexer database test
[confirm('This will run the database tests and requires a local database to be running. Are you sure you want to continue [y/n]?')]
[group('test')]
test-db:
    cargo nextest run --workspace -j1 -E 'test(postgres)' -- --ignored

# Add sqlx migration
[group('db')]
sqlx-add name:
    cargo sqlx migrate add -r {{ name }}

# Run sqlx migrations
[group('db')]
sqlx-migrate:
    cargo sqlx migrate run

# Rollback the last sqlx migration
[group('db')]
sqlx-rollback:
    cargo sqlx migrate revert

# Run sqlx offline preparation
[group('db')]
sqlx-prepare:
    cargo sqlx prepare --workspace -- --all-targets

# Run pre-push checks
[group('dev')]
pre-push: lint-fix test-all sqlx-prepare

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
run-benchmark indexer_url:
    @echo "Running benchmark for indexer URL: {{ indexer_url }} with default config"
    cargo run --release -p data-anchor -- -i {{ indexer_url }} -n bench m a ./target/data

# Clean the programs directory
[group('clean')]
[working-directory('programs')]
clean-programs:
    cargo clean

# Clean the entire project
[group('clean')]
clean: clean-programs
    cargo clean

# Run the geyser plugin locally using the solana-test-validator (linux)
[group('indexer')]
[linux]
[working-directory('crates/indexer/scripts')]
run-geyser: build
    ./linux/run-geyser.sh

# Run the geyser plugin locally using the solana-test-validator (macos)
[group('indexer')]
[macos]
[working-directory('crates/indexer/scripts')]
run-geyser: build
    ./mac/run-geyser.sh

# Run the indexer locally using the yellowstone gRPC plugin (macos)
[group('indexer')]
[macos]
[working-directory('crates/indexer/scripts')]
run-yellowstone:
    ./mac/run-yellowstone.sh

# Run the indexer locally using the yellowstone gRPC plugin (linux)
[group('indexer')]
[linux]
[working-directory('crates/indexer/scripts')]
run-yellowstone:
    ./linux/run-yellowstone.sh

# Run the yellowstone consumer binary
[group('indexer')]
run-yellowstone-consumer url token="":
    cargo run --release --bin yellowstone-consumer -- -y {{ url }} {{ token && "-x " + token }}

# Run the indexer binary
[group('indexer')]
run-indexer rpc-url:
    cargo run --release --bin data-anchor-indexer -- \
        -c postgres://postgres:secret@localhost:5432/postgres \
        -g none \
        -r {{ rpc-url }}

# Run the indexer RPC server
[group('indexer')]
run-rpc:
    cargo run --release --bin data-anchor-rpc -- -c postgres://postgres:secret@localhost:5432/postgres -j '0.0.0.0:9696'

# Run the indexer proof RPC server
[group('indexer')]
run-proof-rpc:
    cargo run --release --bin data-anchor-proof-rpc -- -c postgres://postgres:secret@localhost:5432/postgres -j '0.0.0.0:9697'

# Run sozu reverse proxy over the RPC and proof RPC servers
[group('indexer')]
run-sozu:
    sozu -c scripts/sozu.toml start

# Build the docker image for the indexer
[group('docker')]
[linux]
docker-build:
    docker compose -f ./docker/docker-compose.yml build --ssh default --build-arg PAYER_PUBKEY={{ PAYER_PUBKEY }}

# Run the indexer locally using docker
[group('docker')]
[linux]
docker-run:
    docker compose -f ./docker/docker-compose.yml up --force-recreate

# Run the db locally using docker in detached mode
[group('docker')]
[working-directory('.github/workflows/db')]
docker-run-db:
    docker compose -f ./no-tls-db.yml up -d --wait --force-recreate

# Stop the docker db
[group('docker')]
[working-directory('.github/workflows/db')]
docker-stop-db:
    docker compose -f ./no-tls-db.yml down

[group('tofu')]
[private]
[working-directory('infrastructure')]
initialize-workspace workspace="staging":
    #!/usr/bin/env bash
    set -euxo pipefail

    tofu workspace select {{ workspace }}

# Apply staging infrastructure
[confirm('This will apply the staging infrastructure. Are you sure you want to continue [y/n]?')]
[group('tofu')]
[working-directory('infrastructure')]
apply-staging: initialize-workspace
    #!/usr/bin/env bash
    set -euxo pipefail

    RELEASE=$(git log --pretty=format:'%H' -n 1 origin/main)

    tofu apply \
        -var-file="environments/devnet.tfvars" \
        -var="release_id=${RELEASE}"

# Apply devnet infrastructure
[confirm('This will apply the devnet infrastructure. Are you sure you want to continue [y/n]?')]
[group('tofu')]
[working-directory('infrastructure')]
apply-devnet: (initialize-workspace "devnet")
    #!/usr/bin/env bash
    set -euxo pipefail

    RELEASE=$(git log --pretty=format:'%H' -n 1 origin/devnet)

    tofu apply \
        -var-file="environments/devnet.tfvars" \
        -var="release_id=${RELEASE}"

# Apply mainnet infrastructure
[confirm('This will apply the mainnet infrastructure. Are you sure you want to continue [y/n]?')]
[group('tofu')]
[working-directory('infrastructure')]
apply-mainnet: (initialize-workspace "mainnet")
    #!/usr/bin/env bash
    set -euxo pipefail

    RELEASE=$(git log --pretty=format:'%H' -n 1 origin/mainnet)

    tofu apply \
        -var-file="environments/mainnet.tfvars" \
        -var="release_id=${RELEASE}"

# Run local e2e tests
[confirm('This will run all the indexer components and run CLI commands against it. Are you sure you want to continue [y/n]?')]
[group('test')]
run-e2e prover-mode='' private-key='':
    {{ prover-mode && "SP1_PROVER=" + prover-mode }} {{ private-key && "NETWORK_PRIVATE_KEY=" + private-key }} ./scripts/run-e2e.sh
