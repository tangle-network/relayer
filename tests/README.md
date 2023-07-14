# Relayer Test Suite Instructions

This document provides instructions on how to run the Relayer base test suite and E2E (end-to-end) test suite.

## Prerequisites

Before running the test suites, ensure that you have the following set up:

1. Setting up Tangle:
```bash
    git clone https://github.com/webb-tools/tangle.git
    cd tangle
    cargo build --release --features integration-tests -p tangle-standalone
```

2. Clone the Relayer repository:
```
    git clone https://github.com/webb-tools/relayer.git
```

---
## Running the Test Suites

To run the test suites, perform the following steps:

1. Install Packages
```
    cd tests
    yarn
```

2. Fetch fixtures
```
    yarn fetch-fixtures
```
3. Run E2E tests
```
    yarn test-evm
```



**Note:** The Relayer must be compiled using the `--features integration-tests,cli` flags.

Please make sure that the Tangle and Relayer projects are located next to each other.

## Additional Notes

- The E2E test suite validates the end-to-end functionality of the Relayer.
- For any issues or questions, refer to the project repositories for support.