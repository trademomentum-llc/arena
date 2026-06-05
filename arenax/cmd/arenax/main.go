// cmd/arenax/main.go - CLI root for arenax per TECHNICAL_SPECIFICATION.md
package main

import (
	"flag"
	"fmt"
	"os"
	"os/exec"
	"strings"

	"trademomentum.com/arenax/internal/arena"
	"trademomentum.com/arenax/internal/config"
	"trademomentum.com/arenax/internal/drift"
	"trademomentum.com/arenax/internal/gitx"
	"trademomentum.com/arenax/internal/review"
	"trademomentum.com/arenax/internal/sizebound"
)

func main() {
	if len(os.Args) < 2 {
		usage()
		os.Exit(2)
	}

	cmd := os.Args[1]
	args := os.Args[2:]

	switch cmd {
	case "review-staged":
		runReviewStaged(args)
	case "review-range":
		runReviewRange(args)
	case "drift":
		runDrift(args)
	case "setup":
		runSetup(args)
	case "doctor":
		runDoctor(args)
	case "help", "-h", "--help":
		usage()
	default:
		fmt.Fprintf(os.Stderr, "unknown command: %s\n", cmd)
		usage()
		os.Exit(2)
	}
}

func usage() {
	fmt.Println(`arenax - Arena dev integration wrapper (M2)

Usage:
  arenax review-staged [--backend local|api|council]
  arenax review-range <revA>..<revB> [--backend ...]
  arenax drift [--backend ...]
  arenax setup [--install-hooks] [--uninstall-hooks]
  arenax doctor
  arenax help

See specs for details. Backend defaults to "local" (uses qwen-coder-local via Ollama/etc).`)
}

func runReviewStaged(args []string) {
	fs := flag.NewFlagSet("review-staged", flag.ExitOnError)
	backend := fs.String("backend", "local", "local | api | council")
	fs.Parse(args)

	cfg := mustLoadConfig()
	client := arena.Client{Bin: cfg.ArenaBin, Env: buildEnv(cfg, *backend)}

	diff, err := gitx.StagedDiff(".")
	if err != nil {
		die(5, "no changes or git error: %v", err)
	}
	if strings.TrimSpace(diff) == "" {
		fmt.Println("No staged changes.")
		os.Exit(5)
	}

	bounded, report := sizeBoundIfNeeded(diff, cfg)
	if !report.Fit {
		fmt.Printf("Warning: context truncated (%d bytes dropped)\n", report.DroppedBytes)
	}

	spec := arena.SessionSpec{
		Type:    "code-review",
		Task:    "Review staged changes",
		Context: bounded,
		Workers: resolveWorkers(*backend, cfg),
		Mode:    "human-in-loop",
	}
	if *backend == "council" {
		spec.Mode = "council"
	}

	uuid, err := client.Create(spec)
	if err != nil {
		die(3, "create failed: %v", err)
	}
	fmt.Printf("Session created: %s\n", uuid)

	res, err := client.Run(uuid)
	if err != nil {
		die(4, "run failed: %v", err)
	}
	review.Summarize(os.Stdout, uuid, res)
}

func runReviewRange(args []string) {
	if len(args) < 1 {
		fmt.Fprintln(os.Stderr, "usage: arenax review-range <a>..<b> [--backend B]")
		os.Exit(2)
	}
	rangeSpec := args[0]
	fs := flag.NewFlagSet("review-range", flag.ExitOnError)
	backend := fs.String("backend", "local", "local | api | council")
	fs.Parse(args[1:])

	// parse a..b
	parts := strings.SplitN(rangeSpec, "..", 2)
	if len(parts) != 2 {
		die(2, "invalid range %s, want A..B", rangeSpec)
	}

	cfg := mustLoadConfig()
	client := arena.Client{Bin: cfg.ArenaBin, Env: buildEnv(cfg, *backend)}

	diff, err := gitx.RangeDiff(".", parts[0], parts[1])
	if err != nil {
		die(5, "git range error: %v", err)
	}
	if strings.TrimSpace(diff) == "" {
		fmt.Println("No changes in range.")
		os.Exit(5)
	}

	bounded, _ := sizeBoundIfNeeded(diff, cfg)

	spec := arena.SessionSpec{
		Type:    "code-review",
		Task:    fmt.Sprintf("Review range %s", rangeSpec),
		Context: bounded,
		Workers: resolveWorkers(*backend, cfg),
		Mode:    "human-in-loop",
	}
	if *backend == "council" {
		spec.Mode = "council"
	}

	uuid, err := client.Create(spec)
	if err != nil {
		die(3, "create: %v", err)
	}
	fmt.Printf("Session created: %s\n", uuid)

	res, err := client.Run(uuid)
	if err != nil {
		die(4, "run: %v", err)
	}
	review.Summarize(os.Stdout, uuid, res)
}

func runDrift(args []string) {
	fs := flag.NewFlagSet("drift", flag.ExitOnError)
	backend := fs.String("backend", "local", "local | api | council")
	fs.Parse(args)

	cfg := mustLoadConfig()
	client := arena.Client{Bin: cfg.ArenaBin, Env: buildEnv(cfg, *backend)}

	paths, err := gitx.ChangedFiles(".")
	if err != nil {
		die(5, "git changed files: %v", err)
	}

	specs, impls := drift.Classify(paths)
	specs = drift.ExpandToFiles(specs)
	impls = drift.ExpandToFiles(impls)

	if len(impls) == 0 {
		fmt.Println("No implementation files changed.")
		os.Exit(5)
	}

	agent := "qwen-coder-local"
	if *backend != "local" {
		agent = "gpt-4-turbo" // or from cfg
	}

	res, err := client.DriftCheck(specs, impls, agent)
	if err != nil {
		die(4, "drift-check: %v", err)
	}
	if len(res.Findings) == 0 {
		fmt.Println("No drift detected. Implementations match specs.")
	} else {
		fmt.Printf("Drift findings (%d):\n", len(res.Findings))
		for _, f := range res.Findings {
			fmt.Printf("  [%s] %s\n", f.Severity, f.Description)
		}
	}
}

func runDoctor(args []string) {
	cfg := mustLoadConfig()
	fmt.Println("arenax doctor")
	fmt.Printf("  arena_bin: %s -> %s\n", cfg.ArenaBin, checkBin(cfg.ArenaBin))
	fmt.Printf("  local_endpoint: %s -> %s\n", cfg.LocalEndpoint, checkEndpoint(cfg.LocalEndpoint, cfg.AllowRemoteEndpoint))
	// libmorphlex check (optional for MX)
	if _, err := os.Stat("libs/libmorphlex.so"); err == nil {
		fmt.Println("  libmorphlex.so -> present")
	} else {
		fmt.Println("  libmorphlex.so -> MISSING (MX token count disabled)")
	}
	fmt.Println("  git ->", checkBin("git"))
	fmt.Println("Status: partial (see full spec for complete table).")
	fmt.Println("Tip: --backend mock for fully offline verification (uses arena built-in mocks). --backend local for real local model.")
}

func runSetup(args []string) {
	fmt.Println("arenax setup (skeleton): would locate/build arena, install to PATH, write .env.example, optionally install hooks (reversible).")
	// Full impl in later iteration per M3
}

func mustLoadConfig() *config.Config {
	cfg, err := config.Load()
	if err != nil {
		die(1, "config load: %v", err)
	}
	return cfg
}

func buildEnv(cfg *config.Config, backend string) []string {
	env := os.Environ()
	// For local, ensure the local key if needed (usually "local" dummy)
	if backend == "local" && cfg.LocalAPIKey != "" {
		env = append(env, "ARENA_LOCAL_API_KEY="+cfg.LocalAPIKey)
	}
	return env
}

func resolveWorkers(backend string, cfg *config.Config) []string {
	switch backend {
	case "local":
		return []string{"qwen-coder-local"}
	case "api":
		return []string{"gpt-4-turbo", "claude-3-sonnet"}
	case "council":
		return []string{"gpt-4-turbo", "claude-3-sonnet"} // + council in spec later
	case "mock":
		return []string{"mock-reviewer-1", "mock-reviewer-2"}
	default:
		return []string{"qwen-coder-local"}
	}
}

func sizeBoundIfNeeded(ctx string, cfg *config.Config) (string, sizebound.Report) {
	cap := cfg.MaxContextBytes
	if cap == 0 {
		cap = 40960
	}
	return sizebound.Fit(ctx, cap, cfg.UseMXTokenCount)
}

func checkBin(bin string) string {
	if _, err := os.Stat(bin); err == nil {
		return "found"
	}
	if p, err := exec.LookPath(bin); err == nil {
		return "on PATH: " + p
	}
	return "MISSING"
}

func checkEndpoint(ep string, allowRemote bool) string {
	// In real: try http health or just validate format + loopback
	if strings.HasPrefix(ep, "http://localhost") || strings.HasPrefix(ep, "http://127.0.0.1") || strings.HasPrefix(ep, "http://[::1]") {
		return "loopback OK"
	}
	if allowRemote {
		return "remote (allowed)"
	}
	return "NON-LOOPBACK (will be rejected by guard)"
}

func die(code int, format string, a ...any) {
	fmt.Fprintf(os.Stderr, format+"\n", a...)
	os.Exit(code)
}
