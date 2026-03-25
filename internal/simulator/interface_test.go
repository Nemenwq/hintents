// Copyright 2026 Erst Users
// SPDX-License-Identifier: Apache-2.0

package simulator

import (
	"context"
	"testing"

	"github.com/dotandev/hintents/internal/errors"
	"github.com/dotandev/hintents/internal/rpc"
	"github.com/stretchr/testify/assert"
)

func TestRunnerInterfaceCompileTimeCheck(t *testing.T) {
	// Verify Runner implements RunnerInterface at compile time
	var _ RunnerInterface = (*Runner)(nil)

	// This test ensures the interface contract is maintained
	assert.True(t, true, "Runner implements RunnerInterface")
}

func TestRPCProviderCompileTimeCheck(t *testing.T) {
	// Verify RPCClientAdapter implements RPCProvider at compile time
	var _ RPCProvider = (*RPCClientAdapter)(nil)

	// This test ensures the interface contract is maintained
	assert.True(t, true, "RPCClientAdapter implements RPCProvider")
}

func TestNewRunnerInterface(t *testing.T) {
	// Test the factory function
	runner, err := NewRunnerInterface()

	// Note: This will fail in the current environment due to missing binary
	// but the interface structure is correct
	if err != nil {
		// Expected in test environment without erst-sim binary
		assert.True(t, errors.Is(err, errors.ErrSimulatorNotFound))
	} else {
		// If binary exists, verify interface is returned
		assert.NotNil(t, runner)
		assert.Implements(t, (*RunnerInterface)(nil), runner)
	}
}

func TestExampleUsage(t *testing.T) {
	// Create a mock implementation for testing
	mockRunner := &mockRunnerForTest{}
	ctx := context.Background()

	req := &SimulationRequest{
		EnvelopeXdr:   "test-envelope",
		ResultMetaXdr: "test-meta",
	}

	resp, err := ExampleUsage(ctx, mockRunner, req)

	assert.NoError(t, err)
	assert.NotNil(t, resp)
	assert.Equal(t, "success", resp.Status)
}

func TestRegressionHarnessWithMockRPC(t *testing.T) {
	// Create mock implementations
	mockRunner := &mockRunnerForTest{}
	mockRPC := &mockRPCProvider{}
	
	// Create harness with mocked dependencies
	harness := NewRegressionHarness(mockRunner, mockRPC, 2)
	
	ctx := context.Background()
	
	// Test with a mock transaction
	result := harness.testTransaction(ctx, "test-tx-hash", nil)
	
	// Verify the mock was called and results are as expected
	assert.Equal(t, "test-tx-hash", result.TransactionHash)
	assert.Equal(t, "pass", result.Status)
	assert.Empty(t, result.ErrorMessage)
}

// Simple mock for testing the interface
type mockRunnerForTest struct{}

func (m *mockRunnerForTest) Run(ctx context.Context, req *SimulationRequest) (*SimulationResponse, error) {
	return &SimulationResponse{
		Status: "success",
		Events: []string{"mock-event"},
	}, nil
}

func (m *mockRunnerForTest) Close() error {
	return nil
}

// Mock RPC provider for testing
type mockRPCProvider struct{}

func (m *mockRPCProvider) GetTransaction(ctx context.Context, hash string) (*rpc.TransactionResponse, error) {
	// Return mock transaction data
	return &rpc.TransactionResponse{
		EnvelopeXdr:   "mock-envelope-xdr",
		ResultXdr:     "mock-result-xdr",
		ResultMetaXdr: "mock-result-meta-xdr",
	}, nil
}

func (m *mockRPCProvider) GetLedgerEntries(ctx context.Context, keys []string) (map[string]string, error) {
	// Return mock ledger entries
	entries := make(map[string]string)
	for _, key := range keys {
		entries[key] = "mock-ledger-entry-xdr"
	}
	return entries, nil
}
