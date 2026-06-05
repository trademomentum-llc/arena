// internal/sizebound/bound.go - deterministic context sizing per spec
package sizebound



type Report struct {
	Fit          bool
	DroppedBytes int
	DroppedTokens int // if MX used
}

func Fit(ctx string, capBytes int, useMX bool) (string, Report) {
	if useMX {
		// stub: for now fall to byte; real would call MX via arena or FFI
	}
	if len(ctx) <= capBytes {
		return ctx, Report{Fit: true}
	}
	trunc := ctx[:capBytes]
	// add truncation marker as per phase1
	marker := "\n... [TRUNCATED by arenax sizebound]"
	if len(trunc)+len(marker) < len(ctx) {
		trunc = trunc[:capBytes-len(marker)] + marker
	}
	return trunc, Report{Fit: false, DroppedBytes: len(ctx) - capBytes}
}

// TokenCounter is the hook for MX (FR-12)
type TokenCounter interface {
	Count(s string) int
}

func FitWithCounter(ctx string, tokenBudget int, counter TokenCounter) (string, Report) {
	if counter == nil {
		return ctx, Report{Fit: true}
	}
	n := counter.Count(ctx)
	if n <= tokenBudget {
		return ctx, Report{Fit: true}
	}
	// simplistic truncate; real would be token aware
	ratio := float64(tokenBudget) / float64(n)
	keep := int(float64(len(ctx)) * ratio)
	if keep >= len(ctx) {
		keep = len(ctx) - 1
	}
	return ctx[:keep], Report{Fit: false, DroppedTokens: n - tokenBudget}
}
