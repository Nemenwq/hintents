// Copyright 2025 Erst Users
// SPDX-License-Identifier: Apache-2.0

package rpc

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"sync"

	"github.com/dotandev/hintents/internal/logger"
	"github.com/dotandev/hintents/internal/telemetry"
	"github.com/stellar/go/clients/horizonclient"
	"go.opentelemetry.io/otel/attribute"
)

// Network types for Stellar
type Network string

const (
	Testnet   Network = "testnet"
	Mainnet   Network = "mainnet"
	Futurenet Network = "futurenet"
)

// Horizon URLs for each network
const (
	TestnetHorizonURL   = "https://horizon-testnet.stellar.org/"
	MainnetHorizonURL   = "https://horizon.stellar.org/"
	FuturenetHorizonURL = "https://horizon-futurenet.stellar.org/"
)

// Soroban RPC URLs
const (
	TestnetSorobanURL   = "https://soroban-testnet.stellar.org"
	MainnetSorobanURL   = "https://mainnet.stellar.validationcloud.io/v1/soroban-rpc-demo" // Public demo endpoint
	FuturenetSorobanURL = "https://rpc-futurenet.stellar.org"
)

// Client handles interactions with the Stellar Network
type Client struct {
	HorizonURL string
	Horizon    horizonclient.ClientInterface
	Network    Network
	SorobanURL string
	AltURLs    []string
	mu         sync.RWMutex
	currIndex  int
}

// TransactionResponse contains the raw XDR fields needed for simulation
type TransactionResponse struct {
	EnvelopeXdr   string
	ResultXdr     string
	ResultMetaXdr string
}

// NewClient creates a new RPC client with the specified network
// If network is empty, defaults to Mainnet
func NewClient(net Network) *Client {
	if net == "" {
		net = Mainnet
	}

	var horizonURL string
	var sorobanURL string

	switch net {
	case Testnet:
		horizonURL = TestnetHorizonURL
		sorobanURL = TestnetSorobanURL
	case Futurenet:
		horizonURL = FuturenetHorizonURL
		sorobanURL = FuturenetSorobanURL
	case Mainnet:
		fallthrough
	default:
		horizonURL = MainnetHorizonURL
		sorobanURL = MainnetSorobanURL
	}

	return NewClientWithURLs([]string{horizonURL}, net)
}

// NewClientWithURL creates a new RPC client with a custom Horizon URL
func NewClientWithURL(url string, net Network) *Client {
	return NewClientWithURLs([]string{url}, net)
}

// NewClientWithURLs creates a new RPC client with a list of Horizon URLs for failover
func NewClientWithURLs(urls []string, net Network) *Client {
	if len(urls) == 0 {
		return NewClient(net)
	}

	// Re-use logic to get default Soroban URL if needed
	var sorobanURL string
	switch net {
	case Testnet:
		sorobanURL = TestnetSorobanURL
	case Futurenet:
		sorobanURL = FuturenetSorobanURL
	default:
		sorobanURL = MainnetSorobanURL
	}

	c := &Client{
		HorizonURL: urls[0],
		Horizon: &horizonclient.Client{
			HorizonURL: urls[0],
			HTTP:       http.DefaultClient,
		},
		Network:    net,
		SorobanURL: sorobanURL,
		AltURLs:    urls,
	}
	return c
}

// rotateURL switches to the next available provider URL
func (c *Client) rotateURL() bool {
	c.mu.Lock()
	defer c.mu.Unlock()

	if len(c.AltURLs) <= 1 {
		return false
	}

	c.currIndex = (c.currIndex + 1) % len(c.AltURLs)
	c.HorizonURL = c.AltURLs[c.currIndex]
	c.Horizon = &horizonclient.Client{
		HorizonURL: c.HorizonURL,
		HTTP:       http.DefaultClient,
	}

	logger.Logger.Warn("RPC failover triggered", "new_url", c.HorizonURL)
	return true
}

// GetTransaction fetches the transaction details and full XDR data with automatic failover
func (c *Client) GetTransaction(ctx context.Context, hash string) (*TransactionResponse, error) {
	for attempt := 0; attempt < len(c.AltURLs); attempt++ {
		resp, err := c.getTransactionAttempt(ctx, hash)
		if err == nil {
			return resp, nil
		}

		// Only rotate if this isn't the last possible URL
		if attempt < len(c.AltURLs)-1 {
			logger.Logger.Warn("Retrying with fallback RPC...", "error", err)
			if !c.rotateURL() {
				break
			}
			continue
		}
		return nil, err
	}
	return nil, fmt.Errorf("all RPC endpoints failed")
}

func (c *Client) getTransactionAttempt(ctx context.Context, hash string) (*TransactionResponse, error) {
	tracer := telemetry.GetTracer()
	_, span := tracer.Start(ctx, "rpc_get_transaction")
	span.SetAttributes(
		attribute.String("transaction.hash", hash),
		attribute.String("network", string(c.Network)),
		attribute.String("rpc.url", c.HorizonURL),
	)
	defer span.End()

	logger.Logger.Debug("Fetching transaction details", "hash", hash, "url", c.HorizonURL)

	tx, err := c.Horizon.TransactionDetail(hash)
	if err != nil {
		span.RecordError(err)
		return nil, fmt.Errorf("failed to fetch transaction from %s: %w", c.HorizonURL, err)
	}

	span.SetAttributes(
		attribute.Int("envelope.size_bytes", len(tx.EnvelopeXdr)),
		attribute.Int("result.size_bytes", len(tx.ResultXdr)),
		attribute.Int("result_meta.size_bytes", len(tx.ResultMetaXdr)),
	)

	logger.Logger.Info("Transaction fetched successfully", "hash", hash, "envelope_size", len(tx.EnvelopeXdr), "url", c.HorizonURL)

	return &TransactionResponse{
		EnvelopeXdr:   tx.EnvelopeXdr,
		ResultXdr:     tx.ResultXdr,
		ResultMetaXdr: tx.ResultMetaXdr,
	}, nil
}

type GetLedgerEntriesRequest struct {
	Jsonrpc string        `json:"jsonrpc"`
	ID      int           `json:"id"`
	Method  string        `json:"method"`
	Params  []interface{} `json:"params"`
}

type GetLedgerEntriesResponse struct {
	Jsonrpc string `json:"jsonrpc"`
	ID      int    `json:"id"`
	Result  struct {
		Entries []struct {
			Key                string `json:"key"`
			Xdr                string `json:"xdr"`
			LastModifiedLedger int    `json:"lastModifiedLedgerSeq"`
			LiveUntilLedger    int    `json:"liveUntilLedgerSeq"`
		} `json:"entries"`
		LatestLedger int `json:"latestLedger"`
	} `json:"result"`
	Error *struct {
		Code    int    `json:"code"`
		Message string `json:"message"`
	} `json:"error,omitempty"`
}

// GetLedgerEntries fetches the current state of ledger entries from Soroban RPC with automatic failover
// keys should be a list of base64-encoded XDR LedgerKeys
func (c *Client) GetLedgerEntries(ctx context.Context, keys []string) (map[string]string, error) {
	if len(keys) == 0 {
		return map[string]string{}, nil
	}

	for attempt := 0; attempt < len(c.AltURLs); attempt++ {
		entries, err := c.getLedgerEntriesAttempt(ctx, keys)
		if err == nil {
			return entries, nil
		}

		if attempt < len(c.AltURLs)-1 {
			logger.Logger.Warn("Retrying with fallback Soroban RPC...", "error", err)
			if !c.rotateURL() {
				break
			}
			continue
		}
		return nil, err
	}
	return nil, fmt.Errorf("all Soroban RPC endpoints failed")
}

func (c *Client) getLedgerEntriesAttempt(ctx context.Context, keys []string) (map[string]string, error) {
	logger.Logger.Debug("Fetching ledger entries", "count", len(keys), "url", c.HorizonURL)

	reqBody := GetLedgerEntriesRequest{
		Jsonrpc: "2.0",
		ID:      1,
		Method:  "getLedgerEntries",
		Params:  []interface{}{keys},
	}

	bodyBytes, err := json.Marshal(reqBody)
	if err != nil {
		return nil, fmt.Errorf("failed to marshal request: %w", err)
	}

	// Note: We use the current HorizonURL for Soroban RPC as well if provided via flag,
	// or fallback to default if not. In this implementation, HorizonURL and SorobanRPC
	// are assumed to be handled by the same endpoint or derived from it for simplicity
	// in the fallback rotation.
	targetURL := c.HorizonURL
	if c.Network == Testnet && targetURL == "" {
		targetURL = TestnetSorobanURL
	} else if c.Network == Mainnet && targetURL == "" {
		targetURL = MainnetSorobanURL
	}

	req, err := http.NewRequestWithContext(ctx, "POST", targetURL, bytes.NewBuffer(bodyBytes))
	if err != nil {
		return nil, fmt.Errorf("failed to create request: %w", err)
	}
	req.Header.Set("Content-Type", "application/json")

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("failed to execute request to %s: %w", targetURL, err)
	}
	defer resp.Body.Close()

	respBytes, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, fmt.Errorf("failed to read response: %w", err)
	}

	var rpcResp GetLedgerEntriesResponse
	if err := json.Unmarshal(respBytes, &rpcResp); err != nil {
		return nil, fmt.Errorf("failed to unmarshal response: %w", err)
	}

	if rpcResp.Error != nil {
		return nil, fmt.Errorf("rpc error from %s: %s (code %d)", targetURL, rpcResp.Error.Message, rpcResp.Error.Code)
	}

	entries := make(map[string]string)
	for _, entry := range rpcResp.Result.Entries {
		entries[entry.Key] = entry.Xdr
	}

	logger.Logger.Info("Ledger entries fetched successfully", "found", len(entries), "requested", len(keys), "url", targetURL)

	return entries, nil
}
