// Copyright 2026 Erst Users
// SPDX-License-Identifier: Apache-2.0

package simulator

import (
	"context"

	"github.com/dotandev/hintents/internal/rpc"
)

// RPCProvider defines the interface for RPC operations that can be mocked in tests
type RPCProvider interface {
	// GetTransaction fetches transaction details from the network
	GetTransaction(ctx context.Context, hash string) (*rpc.TransactionResponse, error)
	
	// GetLedgerEntries fetches ledger entries from the network
	GetLedgerEntries(ctx context.Context, keys []string) (map[string]string, error)
}

// RPCClientAdapter wraps the existing rpc.Client to implement RPCProvider
type RPCClientAdapter struct {
	client *rpc.Client
}

// NewRPCClientAdapter creates a new adapter for the existing RPC client
func NewRPCClientAdapter(client *rpc.Client) RPCProvider {
	return &RPCClientAdapter{client: client}
}

// GetTransaction delegates to the underlying RPC client
func (a *RPCClientAdapter) GetTransaction(ctx context.Context, hash string) (*rpc.TransactionResponse, error) {
	return a.client.GetTransaction(ctx, hash)
}

// GetLedgerEntries delegates to the underlying RPC client
func (a *RPCClientAdapter) GetLedgerEntries(ctx context.Context, keys []string) (map[string]string, error) {
	return a.client.GetLedgerEntries(ctx, keys)
}
