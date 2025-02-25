# The Pubkey of the payer account for image building

export PAYER_PUBKEY := "2MCVmcuUcREwQKDS3HazuYctkkbZV3XRMspM5eLWRZUV"

# The budget for the arbtest program in milliseconds

export ARBTEST_BUDGET_MS := "10000"

[private]
fmt-justfile:
    just --fmt --unstable --check

# Run lint and formatting checks for the programs directory
[working-directory('programs')]
lint-programs:
    cargo +nightly fmt -- --check
    cargo clippy --all-targets --all-features
    zepter run check

# Run lint and formatting checks for the entire project
lint: lint-programs fmt-justfile
    cargo +nightly fmt -- --check
    cargo clippy --all-targets --all-features
    zepter

[private]
fmt-justfile-fix:
    just --fmt --unstable

# Fix lint and formatting issues in the programs directory
[working-directory('programs')]
lint-programs-fix:
    cargo +nightly fmt
    cargo clippy --fix --allow-dirty --allow-staged --all-targets --all-features
    zepter

# Fix lint and formatting issues in the entire project
lint-fix: lint-programs-fix fmt-justfile-fix
    cargo +nightly fmt
    cargo clippy --fix --allow-dirty --allow-staged --all-targets --all-features
    zepter

# Run tests for the programs directory
[working-directory('programs')]
test-programs: build-programs
    cargo nextest run --workspace --status-level skip

# Run compute budget tests for transaction fees
test-compute-unit-limit:
    cargo nextest run --workspace -E 'test(compute_unit_limit)' -- --ignored

# Run tests for the entire project
test: test-programs
    cargo nextest run --workspace --status-level skip

# Build the programs
[working-directory('programs')]
build-programs:
    anchor build --no-idl

# Build the entire project
build: build-programs
    cargo build --release

# Deploy the blober program
[confirm('Are you sure you want to deploy the blober program?')]
[working-directory('programs')]
deploy network:
    anchor keys sync --provider.cluster {{ network }}
    anchor build --no-idl
    anchor deploy --provider.cluster {{ network }}

init-blober program_id namespace:
    cargo run -p nitro-da-cli -- -p {{ program_id }} -i ws://localhost:9696 -n {{ namespace }} br i

# Clean the programs directory
[working-directory('programs')]
clean-programs:
    cargo clean

# Clean the entire project
clean: clean-programs
    cargo clean

# Run the indexer locally using the solana-test-validator (linux)
[linux]
[working-directory('crates/indexer/scripts')]
run-indexer: build
    ./run-linux.sh

# Run the indexer locally using the solana-test-validator (macos)
[macos]
[working-directory('crates/indexer/scripts')]
run-indexer: build
    ./run-mac.sh

# Build the docker image for the indexer
[linux]
docker-build:
    docker compose -f ./docker/docker-compose.yml build --ssh default --build-arg PAYER_PUBKEY={{ PAYER_PUBKEY }}

# Run the indexer locally using docker
[linux]
docker-run:
    docker compose -f ./docker/docker-compose.yml up --force-recreate
