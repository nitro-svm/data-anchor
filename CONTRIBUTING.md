# Contributing to Data Anchor

Thank you for your interest in contributing to Data Anchor! This document provides guidelines for contributing to the project.

## Development Setup

Please see [DEVELOPING.md](./DEVELOPING.md) for detailed instructions on:
- Installation requirements
- Building the project
- Running tests
- Development workflow

## How to Contribute

### Reporting Issues

- Use the GitHub issue tracker to report bugs or request features
- Provide clear reproduction steps for bugs
- Include relevant system information and error messages

### Pull Requests

1. Fork the repository
2. Create a feature branch from `main`
3. Make your changes
4. Ensure all tests pass
5. Run mutation testing where applicable (see DEVELOPING.md)
6. Submit a pull request with a clear description of changes

### Code Style

- Follow existing code conventions in the codebase
- Run linting and formatting tools before submitting
- Write clear commit messages

### Testing

- Add tests for new functionality
- Ensure existing tests continue to pass
- For the `proofs` crate, set `ARBTEST_BUDGET_MS=10000` for thorough testing

## Questions?

Feel free to open an issue for questions about contributing or development setup.