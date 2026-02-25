#!/usr/bin/env bash
set -euo pipefail

# Issue #573 — Enable background execution mode for long simulations

# --- Feature files ---

git add internal/simulator/async.go
git commit -m "feat(simulator): add AsyncRunner for background execution mode

Introduce AsyncRunner wrapping RunnerInterface with Submit, Poll, Wait
and Cleanup methods. Jobs execute in goroutines and are tracked by UUID.
Wait supports configurable poll interval and timeout with context
cancellation."

git add internal/simulator/async_test.go
git commit -m "test(simulator): add async runner test coverage

Cover submit+poll, failure propagation, timeout expiry, cleanup,
nonexistent job lookup, and context cancellation."

git add internal/cmd/debug.go
git commit -m "feat(cmd): wire --async and --async-timeout flags into debug command

Add runAsyncSimulation helper using watch.Spinner for terminal feedback.
Branch single-network simulation path to use AsyncRunner when --async is
set. Declare previously-missing flag variables."

# --- Bug-fix files (pre-existing issues) ---

git add internal/simulator/runner.go
git commit -m "fix(simulator): add Validator field and fix pointer receiver on ipc.Error"

git add internal/rpc/verification.go
git commit -m "fix(rpc): remove unused bytes import in verification.go"

git add internal/watch/spinner.go
git commit -m "fix(watch): remove unused fmt import in spinner.go"

git add internal/visualizer/color.go
git commit -m "fix(visualizer): repair duplicate function bodies from bad merge"

git add internal/dwarf/parser.go
git commit -m "fix(dwarf): repair duplicate parseWASM code block from bad merge"

git add internal/cmd/rpc.go
git commit -m "fix(cmd): scope err/cfg variables correctly in rpc health command"

git add internal/cmd/shell.go
git commit -m "fix(cmd): correct NewClient call signature and remove unused import"

# --- Housekeeping ---

git add internal/simulator/helpers_test.go
git commit -m "chore(simulator): add shared uint32Ptr/stringPtr test helpers"

echo ""
echo "All commits created. Ready to push."
