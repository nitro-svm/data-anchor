export PAYER_PUBKEY := "2MCVmcuUcREwQKDS3HazuYctkkbZV3XRMspM5eLWRZUV"

[private]
fmt-justfile:
    just --fmt --unstable --check

# Run lint and formatting checks for the programs directory
[working-directory('programs')]
lint-programs:
    cargo +nightly fmt -- --check
    cargo clippy --all-targets --all-features

# Run lint and formatting checks for the entire project
lint: lint-programs fmt-justfile
    cargo +nightly fmt -- --check
    cargo clippy --all-targets --all-features

[private]
fmt-justfile-fix:
    just --fmt --unstable

# Fix lint and formatting issues in the programs directory
[working-directory('programs')]
lint-programs-fix:
    cargo +nightly fmt
    cargo clippy --fix --allow-dirty --allow-staged --all-targets --all-features

# Fix lint and formatting issues in the entire project
lint-fix: lint-programs-fix fmt-justfile-fix
    cargo +nightly fmt
    cargo clippy --fix --allow-dirty --allow-staged --all-targets --all-features

# Run tests for the programs directory
[working-directory('programs')]
test-programs:
    cargo nextest run

# Run tests for the entire project
test: test-programs
    cargo nextest run

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
deploy:
    anchor deploy

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
    ./run-macos.sh

# Build the docker image for the indexer
[linux]
docker-build:
    docker compose -f ./docker/docker-compose.yml build --ssh default --build-arg PAYER_PUBKEY={{ PAYER_PUBKEY }}

# Run the indexer locally using docker
[linux]
docker-run:
    docker compose -f ./docker/docker-compose.yml up --force-recreate
