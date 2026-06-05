package sizebound

import "fmt"


// TokenCounter is the hook for exact MX token counting (FR-12).
// When nil, sizebound falls back to the conservative bytes/4 heuristic
// (or direct byte cap here). The stub allows wiring libmorphlex later
// without changing callers.
type TokenCounter interface {
	Count(text string) int
}

// Report describes the outcome of the bound operation.
type Report struct {
	Fit          bool
	DroppedBytes int
	DroppedTokens int // when using counter
	Note         string
}

// Fit applies a deterministic size bound.
// If counter != nil: use token count against a budget derived from cap (cap is bytes, we derive ~cap/4 tokens).
// Else: byte cap, truncate from END (tail) if exceeded, append truncation marker.
// Always returns a string whose byte len <= cap when using byte mode (or token budget).
// The returned context is safe to pass as --context (or @file contents).
// This is the single exported entry point for the sizebound package.
//
// Usage:
//
//	bounded, rpt := sizebound.Fit(largeDiff, 40960, nil)
//	if !rpt.Fit { log warning }
func Fit(ctx string, cap int, counter TokenCounter) (string, Report) {
	if cap <= 0 {
		cap = 40960
	}
	if counter != nil {
		// derive token budget conservatively: cap bytes / 4
		budget := cap / 4
		n := counter.Count(ctx)
		if n <= budget {
			return ctx, Report{Fit: true, Note: "exact token count within budget"}
		}
		trunc := truncateToTokens(ctx, budget, counter)
		return trunc, Report{Fit: false, DroppedTokens: n - budget, Note: "truncated to token budget via counter"}
	}
	// byte mode
	if len(ctx) <= cap {
		return ctx, Report{Fit: true}
	}
	trunc := ctx[:cap]
	// append marker at end (may make it slightly over? no, we truncate more to fit marker
	marker := "\n\n[... diff truncated by arenax sizebound; original had " + fmt.Sprintf("%d", len(ctx)-cap) + " more bytes ...]\n"
	room := cap - len(marker)
	if room < 0 {
		room = 0
	}
	if len(ctx) > room {
		trunc = ctx[:room]
	}
	trunc += marker
	return trunc, Report{Fit: false, DroppedBytes: len(ctx) - len(trunc) + len(marker), Note: "byte cap truncation (tail)"}
}

func truncateToTokens(ctx string, budget int, counter TokenCounter) string {
	// naive: binary search or linear drop from tail by lines/bytes until count <= budget
	// for stub, simple: if no counter impl detail, just cap bytes roughly
	if counter == nil {
		return ctx
	}
	// drop suffix until under
	runes := []rune(ctx)
	for len(runes) > 0 {
		cur := string(runes)
		if counter.Count(cur) <= budget {
			return cur
		}
		// drop last 10% or 1 line
		drop := len(runes) / 10
		if drop < 1 {
			drop = 1
		}
		runes = runes[:len(runes)-drop]
	}
	return ""
}

// StubCounter is a trivial implementation for tests (bytes/4).
type StubCounter struct{}

func (StubCounter) Count(s string) int {
	if s == "" {
		return 0
	}
	// approx 4 bytes/token for code
	return len(s) / 4
}
