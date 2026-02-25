# feat: enable background execution mode for long simulations (#573)

## Problem

Heavy simulation trace requests can block the main thread connection and risk
timeouts. Users running complex multi-contract simulations or large ledger
replays had no way to avoid this bottleneck.

## Solution

Introduced an `--async` flag on the `debug` command that submits a simulation
request in the background, polls for completion with exponential back-off, and
returns the result once ready — without blocking the CLI session or risking
connection timeouts.

### New files

| File                                 | Purpose                                                                                                         |
| ------------------------------------ | --------------------------------------------------------------------------------------------------------------- |
| `internal/simulator/async.go`        | `AsyncRunner` wrapping any `RunnerInterface`: `Submit`, `Poll`, `Wait`, `Cleanup`                               |
| `internal/simulator/async_test.go`   | 6 test cases covering submit+poll, failure propagation, timeout, cleanup, nonexistent job, context cancellation |
| `internal/simulator/helpers_test.go` | Shared `uint32Ptr` / `stringPtr` test helpers                                                                   |

### Modified files

| File                           | Change                                                                                                             |
| ------------------------------ | ------------------------------------------------------------------------------------------------------------------ |
| `internal/cmd/debug.go`        | Added `--async` and `--async-timeout` flags; `runAsyncSimulation` function using `watch.Spinner` for user feedback |
| `internal/simulator/runner.go` | Added `Validator` field to `Runner` struct; fixed pointer-receiver call on `ipc.Error`                             |
| `internal/rpc/verification.go` | Removed unused `bytes` import                                                                                      |
| `internal/watch/spinner.go`    | Removed unused `fmt` import                                                                                        |
| `internal/visualizer/color.go` | Fixed duplicate/malformed function bodies from bad merge                                                           |
| `internal/dwarf/parser.go`     | Fixed duplicate code block in `parseWASM` from bad merge                                                           |
| `internal/cmd/rpc.go`          | Fixed out-of-scope `err`/`cfg` variables                                                                           |
| `internal/cmd/shell.go`        | Removed unused `encoding/base64` import; fixed `NewClient` / `NewClientWithURL` call signatures                    |

### Deleted files

| File                                   | Reason                                     |
| -------------------------------------- | ------------------------------------------ |
| `internal/simulator/validator_test.go` | Empty file (0 bytes) causing build failure |

## Usage

```bash
# Run simulation in the background (default 5-minute timeout)
erst debug contract.wasm --async

# Custom timeout
erst debug contract.wasm --async --async-timeout 600
```

## Verification

```bash
# Build
go build ./internal/cmd/ && echo "CMD OK"
go build ./internal/simulator/... && echo "SIMULATOR OK"

# Tests
go test ./internal/simulator/... -count=1
```

## Build proof

![Build proof](attachment)
