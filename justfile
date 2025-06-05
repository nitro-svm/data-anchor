# The Pubkey of the payer account for image building

export PAYER_PUBKEY := "2MCVmcuUcREwQKDS3HazuYctkkbZV3XRMspM5eLWRZUV"

# The budget for the arbtest program in milliseconds

export ARBTEST_BUDGET_MS := "10000"

[group('lint')]
[private]
fmt-justfile:
    just --fmt --unstable --check

# Run lint and formatting checks for the programs directory
[group('lint')]
[working-directory('programs')]
lint-programs:
    cargo +nightly fmt -- --check
    cargo clippy --all-targets --all-features
    zepter run check

# Run lint and formatting checks for the entire project
[group('lint')]
lint: lint-programs fmt-justfile
    cargo +nightly fmt -- --check
    cargo clippy --all-targets --all-features
    zepter

[group('lint')]
[private]
fmt-justfile-fix:
    just --fmt --unstable

# Fix lint and formatting issues in the programs directory
[group('lint')]
[working-directory('programs')]
lint-programs-fix:
    cargo +nightly fmt
    cargo clippy --fix --allow-dirty --allow-staged --all-targets --all-features
    zepter

# Fix lint and formatting issues in the entire project
[group('lint')]
lint-fix: lint-programs-fix fmt-justfile-fix
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
test-with-local: (deploy 'localnet')
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
init-blober program_id namespace:
    cargo run -p data-anchor -- -p {{ program_id }} -i ws://localhost:9696 -n {{ namespace }} br i

[confirm('This will run benchmarks against a deployed program and will take a while. Are you sure you want to continue [y/n]?')]
[group('program-utils')]
run-benchmark program_id indexer_url:
    @echo "Running benchmark for program ID: {{ program_id }} and indexer URL: {{ indexer_url }} with default config"
    cargo run --release -p data-anchor -- -p {{ program_id }} -i {{ indexer_url }} -n bench m a ./target/data

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
run-yellowstone-consumer url token:
    cargo run --bin yellowstone-consumer -- -y {{ url }} -x {{ token }}

# Run the indexer binary
[group('indexer')]
run-indexer rpc-url program-id="CdczmTavZ6HQwSvEgKJtyrQzKYV4MyU6EZ4Gz5KsULoP":
    cargo run --bin data-anchor-indexer -- -c postgres://postgres:secret@localhost:5432/postgres -j '0.0.0.0:9696' -g none -r {{ rpc-url }} -p {{ program-id }}

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
