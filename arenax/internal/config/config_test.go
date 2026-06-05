package config

import (
	"os"
	"path/filepath"
	"testing"
)

func TestDefault_Deterministic(t *testing.T) {
	c1 := Default()
	c2 := Default()
	if c1 != c2 {
		t.Fatal("Default must be deterministic and return identical values")
	}
	if c1.Backend != BackendLocal || c1.MaxContextBytes != 40960 {
		t.Error("unexpected default values")
	}
}

func TestLoad_MissingFileYieldsDefault(t *testing.T) {
	cfg, err := Load("/nonexistent/path/that/does/not/exist/config.yaml")
	if err != nil {
		t.Fatalf("unexpected err: %v", err)
	}
	if cfg.Backend != BackendLocal {
		t.Error("expected default backend")
	}
}

func TestLoad_EmptyPathFallsBackToDefaultLocationLogic(t *testing.T) {
	// even if user dir missing, should not error, just default
	cfg, err := Load("")
	if err != nil {
		t.Fatalf("Load(\"\") err: %v", err)
	}
	if cfg.ArenaBin != "arena" {
		t.Error("bad default")
	}
}

func TestLoad_PartialYAML_Merges(t *testing.T) {
	tmp := t.TempDir()
	p := filepath.Join(tmp, "cfg.yaml")
	data := []byte("backend: api\nmax_context_bytes: 12345\n")
	if err := os.WriteFile(p, data, 0600); err != nil {
		t.Fatal(err)
	}
	cfg, err := Load(p)
	if err != nil {
		t.Fatal(err)
	}
	if cfg.Backend != BackendAPI {
		t.Errorf("expected api backend, got %s", cfg.Backend)
	}
	if cfg.MaxContextBytes != 12345 {
		t.Error("did not override max")
	}
	// absent keys keep default
	if cfg.HookMode != "advisory" {
		t.Error("absent key must keep default")
	}
}

func TestBackendFromString(t *testing.T) {
	tests := []struct {
		in  string
		out Backend
	}{
		{"", BackendLocal},
		{"local", BackendLocal},
		{"api", BackendAPI},
		{"council", BackendCouncil},
		{"bogus", BackendLocal},
	}
	for _, tt := range tests {
		if got := BackendFromString(tt.in); got != tt.out {
			t.Errorf("BackendFromString(%q)=%s want %s", tt.in, got, tt.out)
		}
	}
}

func TestWorkersFor(t *testing.T) {
	c := Default()
	w, m := c.WorkersFor(BackendLocal)
	if w != "qwen-coder-local" || m != "human-in-loop" {
		t.Error("local workers wrong")
	}
	w, m = c.WorkersFor(BackendCouncil)
	if w == "" || m != "council" {
		t.Error("council wrong")
	}
}
