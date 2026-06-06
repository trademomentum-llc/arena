package config

import (
	"os"
	"path/filepath"
	"strconv"
	"strings"

	"trademomentum.com/arenax/internal/arena"
)

// Backend selects the inference workers for a review or drift run.
type Backend = arena.Backend

// Consts for the aliased type (so bare names work in switches etc.).
const (
	BackendLocal   Backend = "local"
	BackendAPI     Backend = "api"
	BackendCouncil Backend = "council"
)

// Config holds arenax settings. All fields have safe defaults.
// This struct and Load are the single exported entry point for the package (per spec guidance).
// Deterministic: missing file or bad keys -> safe defaults (NFR-2).
type Config struct {
	// ArenaBin is the path to the arena executable (resolved on PATH if relative/empty).
	ArenaBin string

	// Backend is the default backend for review/drift commands (overridable by --backend).
	Backend Backend

	// LocalEndpoint is the OpenAI-compatible base_url for local mode.
	LocalEndpoint string

	// LocalRuntime documents the runtime (ollama, mlx, lmstudio) for doctor.
	LocalRuntime string

	// LocalModel is the model tag passed to the local runtime.
	LocalModel string

	// NumCtx is the conservative context window cap (tokens) used to derive byte bound.
	NumCtx int

	// KVCacheType for doctor reporting (q4_0 baseline).
	KVCacheType string

	// MaxContextBytes is the hard byte cap applied by sizebound before @file write (DSN 9.2).
	MaxContextBytes int

	// AllowRemoteEndpoint disables the loopback guard for local base_url (NFR-4).
	AllowRemoteEndpoint bool

	// UseMXTokenCount enables the exact token counter stub/hook (FR-12).
	UseMXTokenCount bool

	// HookMode is "advisory" (never blocks) or "blocking" (council reject blocks).
	HookMode string

	// CouncilThreshold mirrors arena's auto_approve_threshold for blocking hooks.
	CouncilThreshold float64
}

// Default returns the documented defaults (deterministic, no file needed; from TEC 4 + DSN).
func Default() Config {
	return Config{
		ArenaBin:            "arena",
		Backend:             BackendLocal,
		LocalEndpoint:       "http://localhost:11434/v1",
		LocalRuntime:        "ollama",
		LocalModel:          "qwen2.5-coder:7b",
		NumCtx:              16384,
		KVCacheType:         "q4_0",
		MaxContextBytes:     40960,
		AllowRemoteEndpoint: false,
		UseMXTokenCount:     true,
		HookMode:            "advisory",
		CouncilThreshold:    0.9,
	}
}

// Load reads optional --config path (or default ~/.config/arenax/config.yaml).
// Missing file yields defaults (no error). Uses pure stdlib KV parser (C3 determinism, no yaml dep).
// This is the single exported entry point for the config package.
func Load(explicitPath string) (Config, error) {
	p := explicitPath
	if p == "" {
		if dir, err := os.UserConfigDir(); err == nil {
			p = filepath.Join(dir, "arenax", "config.yaml")
		} else {
			return Default(), nil
		}
	}
	data, err := os.ReadFile(p)
	if err != nil {
		if os.IsNotExist(err) {
			return Default(), nil
		}
		return Config{}, err
	}
	kv := parseKV(string(data))
	return fromKV(kv), nil
}

// parseKV: pure stdlib parser (from C3 for minimalism + determinism; no external yaml lib).
func parseKV(s string) map[string]string {
	m := map[string]string{}
	for _, line := range strings.Split(s, "\n") {
		line = strings.TrimSpace(line)
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		if i := strings.Index(line, ":"); i > 0 {
			k := strings.TrimSpace(line[:i])
			v := strings.TrimSpace(line[i+1:])
			v = strings.Trim(v, `"' `)
			m[strings.ToLower(k)] = v
		}
	}
	return m
}

func fromKV(m map[string]string) Config {
	c := Default()
	if v, ok := m["arena_bin"]; ok && v != "" {
		c.ArenaBin = v
	}
	if v, ok := m["backend"]; ok && v != "" {
		c.Backend = Backend(v)
	}
	if v, ok := m["local_endpoint"]; ok && v != "" {
		c.LocalEndpoint = v
	}
	if v, ok := m["local_runtime"]; ok && v != "" {
		c.LocalRuntime = v
	}
	if v, ok := m["local_model"]; ok && v != "" {
		c.LocalModel = v
	}
	if v, ok := m["num_ctx"]; ok && v != "" {
		if n, err := strconv.Atoi(v); err == nil {
			c.NumCtx = n
		}
	}
	if v, ok := m["kv_cache_type"]; ok && v != "" {
		c.KVCacheType = v
	}
	if v, ok := m["max_context_bytes"]; ok && v != "" {
		if n, err := strconv.Atoi(v); err == nil {
			c.MaxContextBytes = n
		}
	}
	if v, ok := m["allow_remote_endpoint"]; ok {
		c.AllowRemoteEndpoint = strings.ToLower(v) == "true"
	}
	if v, ok := m["use_mx_token_count"]; ok {
		c.UseMXTokenCount = strings.ToLower(v) == "true"
	}
	if v, ok := m["hook_mode"]; ok && v != "" {
		c.HookMode = v
	}
	if v, ok := m["council_threshold"]; ok && v != "" {
		if f, err := strconv.ParseFloat(v, 64); err == nil {
			c.CouncilThreshold = f
		}
	}
	return c
}

// BackendFromString normalizes a --backend flag value.
func BackendFromString(s string) Backend {
	switch s {
	case "local", "":
		return BackendLocal
	case "api":
		return BackendAPI
	case "council":
		return BackendCouncil
	default:
		return BackendLocal
	}
}

// WorkersFor returns the comma-separated worker list for Create --workers,
// plus whether council mode is implied.
func (c Config) WorkersFor(b Backend) (workers string, mode string) {
	switch b {
	case BackendLocal:
		return "qwen-coder-local", "human-in-loop"
	case BackendAPI:
		return "gpt-4-turbo,claude-3-sonnet", "human-in-loop"
	case BackendCouncil:
		return "qwen-coder-local,gpt-4-turbo,claude-3-sonnet", "council"
	default:
		return "qwen-coder-local", "human-in-loop"
	}
}
