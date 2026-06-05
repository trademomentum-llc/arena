package review

import (
	"fmt"
	"strings"

	"trademomentum.com/arenax/internal/arena"
	"trademomentum.com/arenax/internal/config"
	"trademomentum.com/arenax/internal/gitx"
	"trademomentum.com/arenax/internal/sizebound"
)

// ReviewResult is the outcome of a review command.
type ReviewResult struct {
	SessionID string
	Summary   string
	RawRun    string
}

// ReviewStaged performs the full review-staged pipeline:
// 1. gitx.StagedDiff
// 2. sizebound.Fit (using cfg.MaxContextBytes, and stub counter if UseMX)
// 3. build SessionSpec with workers from cfg + backend override
// 4. arena.Create + arena.Run
// 5. Summarize
// This is the main exported entry for staged review.
func ReviewStaged(repo string, cfg config.Config, overrideBackend config.Backend, c arena.Client) (ReviewResult, error) {
	diff, err := gitx.StagedDiff(repo)
	if err != nil {
		return ReviewResult{}, err
	}
	if strings.TrimSpace(diff) == "" {
		return ReviewResult{}, arena.ErrNoChanges
	}
	var counter sizebound.TokenCounter
	if cfg.UseMXTokenCount {
		counter = sizebound.StubCounter{}
	}
	bounded, rpt := sizebound.Fit(diff, cfg.MaxContextBytes, counter)
	_ = rpt // could log dropped in verbose

	backend := overrideBackend
	if backend == "" {
		backend = cfg.Backend
	}
	workers, mode := cfg.WorkersFor(backend)

	spec := arena.SessionSpec{
		Type:    "code-review",
		Task:    deriveTaskFromDiff(diff, "staged"),
		Context: bounded,
		Workers: strings.Split(workers, ","),
		Mode:    mode,
	}

	uuid, err := c.Create(spec)
	if err != nil {
		return ReviewResult{}, err
	}
	runRes, err := c.Run(uuid)
	if err != nil {
		return ReviewResult{}, err
	}
	sum := Summarize(runRes.Stdout)
	return ReviewResult{SessionID: uuid, Summary: sum, RawRun: runRes.Stdout}, nil
}

// ReviewRange same as staged but uses RangeDiff.
func ReviewRange(repo, a, b string, cfg config.Config, overrideBackend config.Backend, c arena.Client) (ReviewResult, error) {
	diff, err := gitx.RangeDiff(repo, a, b)
	if err != nil {
		return ReviewResult{}, err
	}
	if strings.TrimSpace(diff) == "" {
		return ReviewResult{}, arena.ErrNoChanges
	}
	var counter sizebound.TokenCounter
	if cfg.UseMXTokenCount {
		counter = sizebound.StubCounter{}
	}
	bounded, rpt := sizebound.Fit(diff, cfg.MaxContextBytes, counter)
	_ = rpt

	backend := overrideBackend
	if backend == "" {
		backend = cfg.Backend
	}
	workers, mode := cfg.WorkersFor(backend)

	spec := arena.SessionSpec{
		Type:    "code-review",
		Task:    deriveTaskFromDiff(diff, fmt.Sprintf("%s..%s", a, b)),
		Context: bounded,
		Workers: strings.Split(workers, ","),
		Mode:    mode,
	}

	uuid, err := c.Create(spec)
	if err != nil {
		return ReviewResult{}, err
	}
	runRes, err := c.Run(uuid)
	if err != nil {
		return ReviewResult{}, err
	}
	sum := Summarize(runRes.Stdout)
	return ReviewResult{SessionID: uuid, Summary: sum, RawRun: runRes.Stdout}, nil
}

func deriveTaskFromDiff(diff, scope string) string {
	// simple deterministic title from first changed file or scope
	lines := strings.Split(diff, "\n")
	for _, l := range lines {
		if strings.HasPrefix(l, "diff --git ") {
			parts := strings.Fields(l)
			if len(parts) > 3 {
				return fmt.Sprintf("Review %s (%s)", parts[3], scope)
			}
		}
	}
	return fmt.Sprintf("Review changes (%s)", scope)
}

// Summarize extracts a user-friendly terminal summary from the arena run stdout.
// It looks for "Responses collected", agent sections, and consistency/council.
// Clear, no secrets.
func Summarize(runStdout string) string {
	var b strings.Builder
	b.WriteString("=== Review Summary ===\n")
	if idx := strings.Index(runStdout, "Responses collected:"); idx != -1 {
		rest := runStdout[idx:]
		// take first few agent blocks
		lines := strings.Split(rest, "\n")
		count := 0
		for _, ln := range lines {
			b.WriteString(ln)
			b.WriteString("\n")
			if strings.HasPrefix(ln, "--- ") {
				count++
			}
			if count > 4 {
				break
			}
		}
	} else {
		// fallback: last 20 lines
		lines := strings.Split(runStdout, "\n")
		start := len(lines) - 20
		if start < 0 {
			start = 0
		}
		for _, ln := range lines[start:] {
			b.WriteString(ln + "\n")
		}
	}
	b.WriteString("\nSession will be visible via: arena view --session-id <id>\n")
	b.WriteString("Human finalize with: arena finalize --session-id <id> --decision approve --reasoning \"...\"\n")
	return b.String()
}
