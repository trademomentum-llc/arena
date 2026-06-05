package review

import (
	"strings"
	"testing"

	"trademomentum.com/arenax/internal/arena"
	"trademomentum.com/arenax/internal/config"
)

func TestSummarize_Extracts(t *testing.T) {
	out := `some noise
Responses collected:

--- mock-reviewer-1 ---
The code looks good.
  [latency: 12ms, cost: $0.0000]

--- mock-reviewer-2 ---
Minor nits.
Consistency score: 0.95
`
	sum := Summarize(out)
	if !strings.Contains(sum, "mock-reviewer-1") || !strings.Contains(sum, "Review Summary") {
		t.Errorf("summary missing content: %s", sum)
	}
}

func TestDeriveTask_Deterministic(t *testing.T) {
	d := "diff --git a/internal/foo/bar.go b/internal/foo/bar.go\n..."
	t1 := deriveTaskFromDiff(d, "staged")
	t2 := deriveTaskFromDiff(d, "staged")
	if t1 != t2 || !strings.Contains(t1, "bar.go") {
		t.Error("task derivation not stable")
	}
}

func TestReviewStaged_NoChanges(t *testing.T) {
	// use a temp non-git? but gitx will error, or empty
	cfg := config.Default()
	c, _ := arena.NewClient("/bin/echo", nil)
	_, err := ReviewStaged("/nonexistent-git-dir-xyz", cfg, "", c)
	if err == nil {
		t.Error("expected err for no repo or no changes")
	}
}

// Note: full end to end review integ is exercised via cmd or arena integ test using mocks.
