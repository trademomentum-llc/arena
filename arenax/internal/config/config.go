package config

import (
	"os"
	"path/filepath"

	"gopkg.in/yaml.v3"
)

// Backend selects the inference workers for a review or drift run.
type Backend string

const (
	// BackendLocal uses the local qwen-coder-local worker (loopback endpoint, no cost).
	BackendLocal Backend = "local"
	// BackendAPI uses hosted API workers (escalation).
	BackendAPI Backend = "api"
	// BackendCouncil uses council mode (workers + evaluators + consensus).
	BackendCouncil Backend = "council"
)

// Config holds arenax settings. All fields have safe defaults.
type Config struct {
	// ArenaBin is the path to the arena executable (resolved on PATH if relative/empty).
	ArenaBin string `yaml:"arena_bin"`

	// Backend is the default backend for review/drift commands (overridable by --backend).
	Backend Backend `yaml:"backend"`

	// LocalEndpoint is the OpenAI-compatible base_url for local mode.
	LocalEndpoint string `yaml:"local_endpoint"`

	// LocalRuntime documents the runtime (ollama, mlx, lmstudio) for doctor.
	LocalRuntime string `yaml:"local_runtime"`

	// LocalModel is the model tag passed to the local runtime.
	LocalModel string `yaml:"local_model"`

	// NumCtx is the conservative context window cap (tokens) used to derive byte bound.
	NumCtx int `yaml:"num_ctx"`

	// KVCacheType for doctor reporting (q4_0 baseline).
	KVCacheType string `yaml:"kv_cache_type"`

	// MaxContextBytes is the hard byte cap applied by sizebound before @file write (DSN 9.2).
	MaxContextBytes int `yaml:"max_context_bytes"`

	// AllowRemoteEndpoint disables the loopback guard for local base_url (NFR-4).
	AllowRemoteEndpoint bool `yaml:"allow_remote_endpoint"`

	// UseMXTokenCount enables the exact token counter stub/hook (FR-12).
	UseMXTokenCount bool `yaml:"use_mx_token_count"`

	// HookMode is "advisory" (never blocks) or "blocking" (council reject blocks).
	HookMode string `yaml:"hook_mode"`

	// CouncilThreshold mirrors arena's auto_approve_threshold for blocking hooks.
	CouncilThreshold float64 `yaml:"council_threshold"`
}

// Default returns the documented defaults (deterministic, no file needed).
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

// Load reads the config file at path (if non-empty and exists). Missing file or
// empty path yields Default(). Partial files are merged over defaults (absent
// keys keep their default values because we pre-populate before unmarshal).
// This is the single exported entry point for the config package.
func Load(path string) (Config, error) {
	cfg := Default()
	if path == "" {
		// try default user location ~/.config/arenax/config.yaml
		if dir, err := os.UserConfigDir(); err == nil {
			path = filepath.Join(dir, "arenax", "config.yaml")
		} else {
			return cfg, nil
		}
	}
	data, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return cfg, nil
		}
		return cfg, err
	}
	if err := yaml.Unmarshal(data, &cfg); err != nil {
		return cfg, err
	}
	return cfg, nil
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
