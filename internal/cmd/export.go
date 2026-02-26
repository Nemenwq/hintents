// Copyright 2025 Erst Users
// SPDX-License-Identifier: Apache-2.0

package cmd

import (
	"encoding/json"
	"fmt"

	"github.com/dotandev/hintents/internal/errors"
	"github.com/dotandev/hintents/internal/simulator"
	"github.com/dotandev/hintents/internal/snapshot"
	"github.com/spf13/cobra"
)

var exportSnapshotFlag string
var exportIncludeMemoryFlag bool

var exportCmd = &cobra.Command{
	Use:     "export",
	GroupID: "utility",
	Short:   "Export data from the current session",
	Long:    `Export debugging data, such as state snapshots, from the currently active session.`,
	RunE: func(cmd *cobra.Command, args []string) error {
		if exportSnapshotFlag == "" {
			return errors.WrapCliArgumentRequired("snapshot")
		}

		// Get current session
		data := GetCurrentSession()
		if data == nil {
			return errors.WrapSimulationLogicError("no active session. Run 'erst debug <tx-hash>' first")
		}

		// Unwrap simulation request to get ledger entries
		var simReq simulator.SimulationRequest
		if err := json.Unmarshal([]byte(data.SimRequestJSON), &simReq); err != nil {
			return errors.WrapUnmarshalFailed(err, "session data")
		}

		if len(simReq.LedgerEntries) == 0 {
			fmt.Println("Warning: No ledger entries found in the current session.")
		}

		// Convert to snapshot
		snapOptions := snapshot.BuildOptions{}
		if exportIncludeMemoryFlag {
			memoryB64, err := extractLinearMemoryBase64(data.SimResponseJSON)
			if err != nil {
				return errors.WrapValidationError(fmt.Sprintf("failed to parse simulation response: %v", err))
			}
			snapOptions.LinearMemoryBase64 = memoryB64
			if memoryB64 == "" {
				fmt.Println("Warning: No linear memory dump found in simulation response.")
			}
		}

		snap := snapshot.FromMapWithOptions(simReq.LedgerEntries, snapOptions)

		// Save
		if err := snapshot.Save(exportSnapshotFlag, snap); err != nil {
			return errors.WrapValidationError(fmt.Sprintf("failed to save snapshot: %v", err))
		}

		fmt.Printf("Snapshot exported to %s (%d entries)\n", exportSnapshotFlag, len(snap.LedgerEntries))
		return nil
	},
}

func init() {
	exportCmd.Flags().StringVar(&exportSnapshotFlag, "snapshot", "", "Output file for JSON snapshot")
	exportCmd.Flags().BoolVar(&exportIncludeMemoryFlag, "include-memory", false, "Include Wasm linear memory dump in snapshot when available")
	rootCmd.AddCommand(exportCmd)
}

func extractLinearMemoryBase64(simResponseJSON string) (string, error) {
	if simResponseJSON == "" {
		return "", nil
	}

	var payload struct {
		LinearMemoryBase64 string `json:"linear_memory_base64"`
		LinearMemory       string `json:"linear_memory"`
	}

	if err := json.Unmarshal([]byte(simResponseJSON), &payload); err != nil {
		return "", err
	}

	if payload.LinearMemoryBase64 != "" {
		return payload.LinearMemoryBase64, nil
	}

	return payload.LinearMemory, nil
}
