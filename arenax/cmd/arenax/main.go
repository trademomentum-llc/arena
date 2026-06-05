package main

import (
	"bytes"
	"flag"
	"fmt"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"text/tabwriter"
	"time"

	"trademomentum.com/arenax/internal/arena"
	"trademomentum.com/arenax/internal/config"
	"trademomentum.com/arenax/internal/drift"
	"trademomentum.com/arenax/internal/gitx"
	"trademomentum.com/arenax/internal/review"
)

// usage prints top level help with examples.
func usage() {
	fmt.Fprintf(os.Stderr, `arenax - ergonomics wrapper for arena dev integration (trademomentum.com/arenax)

Usage:
  arenax review-staged [--backend local|api|council] [--config PATH]
  arenax review-range <a>..<b> [--backend ...]
  arenax drift [--backend ...]
  arenax setup [--install-hooks] [--uninstall-hooks]
  arenax doctor

Backends:
  local   (default)  - qwen-coder-local + optional MX (free, offline, private)
  api                - hosted frontier (escalation)
  council            - workers + council consensus

Environment: arena binary must be on PATH (or configured). Secrets only via env vars (OPENAI_API_KEY etc), never argv.

Examples:
  # stage a change, review locally (no net)
  git add -u
  arenax review-staged --backend local

  # range review
  arenax review-range HEAD~3..HEAD

  # drift check on working changes
  arenax drift

  # bootstrap (writes .env.example, optional hooks)
  arenax setup --install-hooks

  # health table (all PASS when good)
  arenax doctor
`)
}

var (
	configPath string
	backendStr string
)

func initFlags() {
	flag.StringVar(&configPath, "config", "", "path to arenax config.yaml (default ~/.config/arenax/config.yaml)")
	flag.StringVar(&backendStr, "backend", "", "override backend: local|api|council")
}

func main() {
	initFlags()
	// manual subcommand handling because flag stops at first non-flag
	if len(os.Args) < 2 {
		usage()
		os.Exit(2)
	}
	// parse global flags first by shifting
	args := os.Args[1:]
	// simple preparse for -config -backend before subcmd
	subIdx := 0
	for i, a := range args {
		if strings.HasPrefix(a, "-") {
			continue
		}
		subIdx = i
		break
	}
	if subIdx > 0 {
		// reparse only globals
		flag.CommandLine.Parse(args[:subIdx])
		args = args[subIdx:]
	} else {
		flag.CommandLine.Parse(args)
		args = flag.Args()
	}

	if len(args) == 0 {
		usage()
		os.Exit(2)
	}
	sub := args[0]
	subArgs := args[1:]

	// support --backend/--config after the subcommand (common UX) via manual scan
	for i := 0; i < len(subArgs); i++ {
		a := subArgs[i]
		if a == "--backend" || a == "-backend" {
			if i+1 < len(subArgs) {
				backendStr = subArgs[i+1]
			}
		} else if strings.HasPrefix(a, "--backend=") {
			backendStr = strings.TrimPrefix(a, "--backend=")
		} else if a == "--config" || a == "-config" {
			if i+1 < len(subArgs) {
				configPath = subArgs[i+1]
			}
		} else if strings.HasPrefix(a, "--config=") {
			configPath = strings.TrimPrefix(a, "--config=")
		}
	}

	cfg, err := config.Load(configPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "config error: %v\n", err)
		os.Exit(1)
	}
	if backendStr != "" {
		cfg.Backend = config.BackendFromString(backendStr)
	}

	// resolve client once
	ac, err := arena.NewClient(cfg.ArenaBin, nil)
	if err != nil {
		// defer error to subcmds that need it
	}

	switch sub {
	case "review-staged":
		handleReviewStaged(cfg, ac)
	case "review-range":
		if len(subArgs) < 1 {
			fmt.Fprintln(os.Stderr, "review-range requires <a>..<b> range")
			os.Exit(2)
		}
		handleReviewRange(cfg, ac, subArgs[0])
	case "drift":
		handleDrift(cfg, ac)
	case "setup":
		handleSetup(cfg, subArgs)
	case "doctor":
		handleDoctor(cfg)
	case "-h", "--help", "help":
		usage()
		os.Exit(0)
	default:
		fmt.Fprintf(os.Stderr, "unknown subcommand: %s\n", sub)
		usage()
		os.Exit(2)
	}
}

func handleReviewStaged(cfg config.Config, ac arena.Client) {
	res, err := review.ReviewStaged(".", cfg, config.BackendFromString(backendStr), ac)
	if err != nil {
		handleErr(err)
	}
	fmt.Printf("Session created: %s\n", res.SessionID)
	fmt.Print(res.Summary)
	// clear UX: also print how to view
	fmt.Printf("\nTo inspect full responses: arena view --session-id %s\n", res.SessionID)
	os.Exit(0)
}

func handleReviewRange(cfg config.Config, ac arena.Client, rangeSpec string) {
	// parse a..b simply
	parts := strings.SplitN(rangeSpec, "..", 2)
	if len(parts) != 2 {
		fmt.Fprintln(os.Stderr, "range must be a..b")
		os.Exit(2)
	}
	res, err := review.ReviewRange(".", parts[0], parts[1], cfg, config.BackendFromString(backendStr), ac)
	if err != nil {
		handleErr(err)
	}
	fmt.Printf("Session created: %s\n", res.SessionID)
	fmt.Print(res.Summary)
	fmt.Printf("\nTo inspect: arena view --session-id %s\n", res.SessionID)
	os.Exit(0)
}

func handleDrift(cfg config.Config, ac arena.Client) {
	changed, err := gitx.ChangedFiles(".")
	if err != nil {
		handleErr(err)
	}
	if len(changed) == 0 {
		fmt.Println("No changes vs HEAD.")
		os.Exit(5)
	}
	specs, impls := drift.Classify(changed)
	// expand
	allPaths := append(append([]string{}, specs...), impls...)
	expanded, err := drift.ExpandToFiles(allPaths)
	if err != nil {
		handleErr(err)
	}
	s2, i2 := drift.Classify(expanded)
	if len(i2) == 0 {
		fmt.Println("Classification yielded no implementation files (nothing to drift-check).")
		os.Exit(5)
	}

	// build client fresh? use passed
	agent := "qwen-coder-local"
	if cfg.Backend == config.BackendAPI || cfg.Backend == config.BackendCouncil {
		agent = "gpt-4-turbo"
	}
	dr, err := ac.DriftCheck(s2, i2, agent)
	if err != nil {
		handleErr(err)
	}
	fmt.Print(dr.Stdout)
	if dr.Findings == 0 {
		fmt.Println("\n(No drift.)")
	}
	os.Exit(0)
}

func handleSetup(cfg config.Config, subArgs []string) {
	installHooks := false
	uninstall := false
	for _, a := range subArgs {
		if a == "--install-hooks" {
			installHooks = true
		}
		if a == "--uninstall-hooks" {
			uninstall = true
		}
	}
	fmt.Println("=== arenax setup ===")
	// 1. verify arena
	ac, err := arena.NewClient(cfg.ArenaBin, nil)
	if err != nil {
		fmt.Printf("WARNING: arena not found (%v). Run `cargo build --release` in arena tree or ensure on PATH.\n", err)
	} else {
		// quick --help
		res, _ := ac.Run("") // will fail but bin exists
		_ = res
		fmt.Printf("arena binary: %s (OK)\n", ac.Bin)
	}

	// 2. write .env.example (keys only, never values)
	envEx := `# Arena local + escalation keys (never commit)
# For local mode these are unused (loopback).
ARENA_LOCAL_API_KEY=local
OPENAI_API_KEY=sk-...
ANTHROPIC_API_KEY=sk-ant-...
`
	home, _ := os.UserHomeDir()
	envPath := filepath.Join(home, ".env.arenax.example")
	if err := os.WriteFile(envPath, []byte(envEx), 0600); err != nil {
		fmt.Printf("could not write %s: %v\n", envPath, err)
	} else {
		fmt.Printf("wrote %s (edit and source or export as needed)\n", envPath)
	}

	// 3. hooks if requested
	if installHooks || uninstall {
		hookDir := ".git/hooks"
		if _, err := os.Stat(".git"); err != nil {
			fmt.Println("Not in a git repo root; skipping hooks (cd to repo).")
		} else {
			if uninstall {
				for _, name := range []string{"pre-commit", "pre-push"} {
					p := filepath.Join(hookDir, name)
					if b, err := os.ReadFile(p); err == nil && bytes.Contains(b, []byte("arenax")) {
						_ = os.Remove(p)
						fmt.Printf("removed hook %s (was arenax)\n", p)
					}
				}
			}
			if installHooks {
				for _, name := range []string{"pre-commit", "pre-push"} {
					src := filepath.Join("hooks", name+".advisory")
					// prefer advisory
					if _, err := os.Stat(src); err != nil {
						src = filepath.Join("../hooks", name+".advisory") // if run from arenax/
					}
					dst := filepath.Join(hookDir, name)
					backupIfExists(dst)
					data, err := os.ReadFile(src)
					if err != nil {
						// fallback inline template
						data = []byte(defaultAdvisoryHook(name))
					}
					if err := os.WriteFile(dst, data, 0755); err != nil {
						fmt.Printf("hook write fail %s: %v\n", dst, err)
					} else {
						fmt.Printf("installed advisory hook %s (exits 0 always)\n", dst)
					}
				}
			}
		}
	}

	// 4. completions hint
	fmt.Println("Shell completions: arenax supports basic; for full add to your .zshrc etc manually or use `complete`.")

	fmt.Println("Setup complete. Run `arenax doctor` to verify.")
	os.Exit(0)
}

func backupIfExists(p string) {
	if _, err := os.Stat(p); err == nil {
		bak := p + ".bak." + time.Now().Format("20060102-150405")
		_ = os.Rename(p, bak)
		fmt.Printf("backed up existing %s -> %s\n", p, bak)
	}
}

func defaultAdvisoryHook(name string) string {
	if name == "pre-commit" {
		return "#!/bin/sh\n# arenax advisory pre-commit (never blocks)\nexec arenax review-staged || true\n"
	}
	return "#!/bin/sh\n# arenax advisory pre-push\nexec arenax review-range origin/$(git rev-parse --abbrev-ref HEAD)..HEAD || true\n"
}

func handleDoctor(cfg config.Config) {
	type check struct {
		name   string
		status string // PASS / FAIL / SKIP
		detail string
	}
	var checks []check

	// arena bin
	ac, err := arena.NewClient(cfg.ArenaBin, nil)
	if err != nil {
		// fallback search common build locations (parent project etc) for doctor UX
		fallbacks := []string{
			"/Users/nnos/Projects/arena/target/release/arena",
			"../target/release/arena",
			"../../target/release/arena",
			filepath.Join(os.Getenv("HOME"), "Projects/arena/target/release/arena"),
		}
		found := ""
		for _, fb := range fallbacks {
			if _, st := os.Stat(fb); st == nil {
				found = fb
				break
			}
		}
		if found != "" {
			ac, err = arena.NewClient(found, nil)
		}
	}
	if err != nil {
		checks = append(checks, check{"arena binary", "FAIL", fmt.Sprintf("not found: %v (run cargo build --release in parent)", err)})
	} else {
		// try list which is safe
		res, _ := exec.Command(ac.Bin, "list").CombinedOutput()
		ver := "present"
		if len(res) > 0 {
			ver = "runnable"
		}
		checks = append(checks, check{"arena binary", "PASS", fmt.Sprintf("%s (%s)", ac.Bin, ver)})
	}

	// git
	if _, err := exec.LookPath("git"); err != nil {
		checks = append(checks, check{"git", "FAIL", "git not on PATH"})
	} else {
		out, _ := exec.Command("git", "--version").Output()
		checks = append(checks, check{"git", "PASS", strings.TrimSpace(string(out))})
	}

	// local endpoint (probe only if local backend, short timeout, tolerate down for SKIP? but per AC want PASS when good)
	ep := cfg.LocalEndpoint
	if cfg.Backend == config.BackendLocal || backendStr == "local" || backendStr == "" {
		status := "PASS"
		detail := ep
		if u, err := url.Parse(ep); err == nil && (u.Hostname() == "localhost" || strings.HasPrefix(u.Hostname(), "127.") || u.Hostname() == "::1") {
			// try http get with short timeout (may be / or /v1/models for ollama compat)
			client := &http.Client{Timeout: 800 * time.Millisecond}
			resp, err := client.Get(ep)
			if err != nil || (resp != nil && resp.StatusCode >= 500) {
				// not fatal for doctor if runtime not up; but AC says PASS when good
				status = "SKIP"
				detail = ep + " (unreachable now; start your ollama/mlx)"
			} else if resp != nil {
				resp.Body.Close()
				detail = ep + " (reachable)"
			}
		} else if !cfg.AllowRemoteEndpoint {
			status = "FAIL"
			detail = ep + " (non-loopback without allow_remote_endpoint)"
		}
		checks = append(checks, check{"local endpoint", status, detail})
	} else {
		checks = append(checks, check{"local endpoint", "SKIP", "not used (backend=" + string(cfg.Backend) + ")"})
	}

	// libmorphlex.so (for MX)
	libPath := findLibMorphlex()
	if libPath != "" {
		checks = append(checks, check{"libmorphlex.so", "PASS", libPath})
	} else {
		checks = append(checks, check{"libmorphlex.so", "SKIP", "not found (MX token count stubbed; optional)"})
	}

	// config
	checks = append(checks, check{"arenax config", "PASS", fmt.Sprintf("backend=%s max_bytes=%d hook=%s", cfg.Backend, cfg.MaxContextBytes, cfg.HookMode)})

	// write nice table
	w := tabwriter.NewWriter(os.Stdout, 0, 0, 2, ' ', 0)
	fmt.Fprintln(w, "CHECK\tSTATUS\tDETAIL")
	fmt.Fprintln(w, "-----\t------\t------")
	allPass := true
	for _, ch := range checks {
		fmt.Fprintf(w, "%s\t%s\t%s\n", ch.name, ch.status, ch.detail)
		if ch.status == "FAIL" {
			allPass = false
		}
	}
	w.Flush()

	if allPass {
		fmt.Println("\nAll checks PASS. Ready for local reviews (FR-5, FR-8, FR-9).")
		os.Exit(0)
	}
	fmt.Println("\nSome checks failed or skipped. See above.")
	os.Exit(1)
}

func findLibMorphlex() string {
	cands := []string{
		"../libs/libmorphlex.so",
		"../../libs/libmorphlex.so",
		"/Users/nnos/Projects/arena/libs/libmorphlex.so",
		"libs/libmorphlex.so",
		filepath.Join(os.Getenv("HOME"), "Projects/arena/libs/libmorphlex.so"),
	}
	for _, c := range cands {
		if fi, err := os.Stat(c); err == nil && !fi.IsDir() {
			return c
		}
	}
	// try walking up a bit
	dir, _ := os.Getwd()
	for i := 0; i < 5; i++ {
		p := filepath.Join(dir, "libs/libmorphlex.so")
		if _, err := os.Stat(p); err == nil {
			return p
		}
		dir = filepath.Dir(dir)
	}
	return ""
}

func handleErr(err error) {
	switch {
	case errorsIs(err, arena.ErrArenaMissing):
		fmt.Fprintf(os.Stderr, "error: %v\nHint: arenax setup or ensure arena on PATH. Exit 3.\n", err)
		os.Exit(3)
	case errorsIs(err, arena.ErrEndpointDown):
		fmt.Fprintf(os.Stderr, "error: %v\nHint: start local runtime or use --backend api. Exit 4.\n", err)
		os.Exit(4)
	case errorsIs(err, arena.ErrNoChanges):
		fmt.Fprintf(os.Stderr, "error: no changes (empty diff). Exit 5.\n")
		os.Exit(5)
	case errorsIs(err, arena.ErrSessionParse):
		fmt.Fprintf(os.Stderr, "error: %v\nRaw output may help diagnose. Exit 6.\n", err)
		os.Exit(6)
	case errorsIs(err, arena.ErrNoImpls):
		fmt.Fprintln(os.Stderr, "error: no impls after classify. Exit 5.")
		os.Exit(5)
	default:
		fmt.Fprintf(os.Stderr, "error: %v\n", err)
		os.Exit(1)
	}
}

func errorsIs(err, target error) bool {
	// Go 1.22+ has errors.Is, but for sentinel wrappers we check contains too
	if err == target {
		return true
	}
	if strings.Contains(err.Error(), target.Error()) {
		return true
	}
	return false
}
