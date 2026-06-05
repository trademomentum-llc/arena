// internal/config/config.go - load arenax config per TECHNICAL_SPECIFICATION 4
package config

import (
	"os"
	"path/filepath"

	"gopkg.in/yaml.v3" // small dep for yaml; stdlib alternative possible but yaml common
)

type Config struct {
	ArenaBin               string `yaml:"arena_bin"`
	Backend                string `yaml:"backend"`
	LocalEndpoint          string `yaml:"local_endpoint"`
	LocalRuntime           string `yaml:"local_runtime"`
	LocalModel             string `yaml:"local_model"`
	NumCtx                 int    `yaml:"num_ctx"`
	KVCacheType            string `yaml:"kv_cache_type"`
	MaxContextBytes        int    `yaml:"max_context_bytes"`
	AllowRemoteEndpoint    bool   `yaml:"allow_remote_endpoint"`
	UseMXTokenCount        bool   `yaml:"use_mx_token_count"`
	HookMode               string `yaml:"hook_mode"`
	CouncilThreshold       float64 `yaml:"council_threshold"`
	LocalAPIKey            string `yaml:"-"` // not in yaml usually
}

func Default() *Config {
	return &Config{
		ArenaBin:            "arena",
		Backend:             "local",
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

func Load() (*Config, error) {
	cfg := Default()

	if ab := os.Getenv("ARENA_BIN"); ab != "" {
		cfg.ArenaBin = ab
	}

	// Try standard locations
	paths := []string{
		os.Getenv("ARENAX_CONFIG"),
		filepath.Join(os.Getenv("HOME"), ".config/arenax/config.yaml"),
		filepath.Join(os.Getenv("HOME"), ".arenax.yaml"),
	}
	for _, p := range paths {
		if p == "" {
			continue
		}
		if data, err := os.ReadFile(p); err == nil {
			var disk Config
			if yaml.Unmarshal(data, &disk) == nil {
				merge(cfg, &disk)
			}
			break
		}
	}
	return cfg, nil
}

func merge(base, override *Config) {
	if override.ArenaBin != "" {
		base.ArenaBin = override.ArenaBin
	}
	if override.Backend != "" {
		base.Backend = override.Backend
	}
	if override.LocalEndpoint != "" {
		base.LocalEndpoint = override.LocalEndpoint
	}
	// add more fields as needed...
	if override.AllowRemoteEndpoint {
		base.AllowRemoteEndpoint = true
	}
}
