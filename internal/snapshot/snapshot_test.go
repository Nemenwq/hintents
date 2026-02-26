// Copyright 2025 Erst Users
// SPDX-License-Identifier: Apache-2.0

package snapshot

import (
	"encoding/base64"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestFromMapSortsByKey(t *testing.T) {
	snap := FromMap(map[string]string{
		"key-c": "value-c",
		"key-a": "value-a",
		"key-b": "value-b",
	})

	if got, want := len(snap.LedgerEntries), 3; got != want {
		t.Fatalf("expected %d entries, got %d", want, got)
	}

	if snap.LedgerEntries[0][0] != "key-a" {
		t.Fatalf("expected first key key-a, got %s", snap.LedgerEntries[0][0])
	}
	if snap.LedgerEntries[1][0] != "key-b" {
		t.Fatalf("expected second key key-b, got %s", snap.LedgerEntries[1][0])
	}
	if snap.LedgerEntries[2][0] != "key-c" {
		t.Fatalf("expected third key key-c, got %s", snap.LedgerEntries[2][0])
	}
}

func TestSaveNormalizesEntryOrder(t *testing.T) {
	snap := &Snapshot{
		LedgerEntries: []LedgerEntryTuple{
			{"key-z", "value-z"},
			{"key-a", "value-a"},
			{"key-m", "value-m"},
		},
	}

	outPath := filepath.Join(t.TempDir(), "snapshot.json")
	if err := Save(outPath, snap); err != nil {
		t.Fatalf("Save failed: %v", err)
	}

	data, err := os.ReadFile(outPath)
	if err != nil {
		t.Fatalf("failed to read saved snapshot: %v", err)
	}

	text := string(data)
	posA := strings.Index(text, "\"key-a\"")
	posM := strings.Index(text, "\"key-m\"")
	posZ := strings.Index(text, "\"key-z\"")
	if posA == -1 || posM == -1 || posZ == -1 {
		t.Fatalf("saved JSON does not contain expected keys: %s", text)
	}
	if !(posA < posM && posM < posZ) {
		t.Fatalf("expected keys to be sorted in saved JSON, got: %s", text)
	}
}

func TestSaveNilSnapshot(t *testing.T) {
	outPath := filepath.Join(t.TempDir(), "nil-snapshot.json")
	if err := Save(outPath, nil); err != nil {
		t.Fatalf("Save failed for nil snapshot: %v", err)
	}

	data, err := os.ReadFile(outPath)
	if err != nil {
		t.Fatalf("failed to read saved snapshot: %v", err)
	}
	if strings.TrimSpace(string(data)) == "" {
		t.Fatal("expected non-empty JSON for nil snapshot")
	}
}

func TestFromMapWithOptions_LinearMemory(t *testing.T) {
	memory := []byte("hello-memory")
	encoded := base64.StdEncoding.EncodeToString(memory)

	snap := FromMapWithOptions(map[string]string{"a": "b"}, BuildOptions{LinearMemoryBase64: encoded})
	if snap.LinearMemoryBase64 != encoded {
		t.Fatalf("expected encoded memory to be preserved")
	}

	decoded, err := snap.DecodeLinearMemory()
	if err != nil {
		t.Fatalf("DecodeLinearMemory returned error: %v", err)
	}
	if string(decoded) != string(memory) {
		t.Fatalf("unexpected decoded memory: %q", string(decoded))
	}
}

func TestSave_StoresLinearMemorySize(t *testing.T) {
	memory := []byte{0x01, 0x02, 0x03, 0x04}
	encoded := base64.StdEncoding.EncodeToString(memory)
	snap := &Snapshot{LedgerEntries: []LedgerEntryTuple{}, LinearMemoryBase64: encoded}

	outPath := filepath.Join(t.TempDir(), "snapshot-memory.json")
	if err := Save(outPath, snap); err != nil {
		t.Fatalf("Save failed: %v", err)
	}

	raw, err := os.ReadFile(outPath)
	if err != nil {
		t.Fatalf("failed reading snapshot: %v", err)
	}
	text := string(raw)
	if !strings.Contains(text, `"linearMemorySize": 4`) {
		t.Fatalf("expected linearMemorySize in saved snapshot, got: %s", text)
	}
}
