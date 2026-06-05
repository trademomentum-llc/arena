// internal/review/review.go
package review

import (
	"fmt"
	"io"

	"trademomentum.com/arenax/internal/sizebound"
)

type BoundReport = sizebound.Report

func Summarize(w io.Writer, uuid string, res struct {
	Stdout   string
	ExitCode int
}) {
	fmt.Fprintf(w, "Session %s completed (exit %d).\n", uuid, res.ExitCode)
	// In real: pretty print the responses from arena view or captured output
	fmt.Fprintln(w, "Responses captured. Use 'arena view --session-id "+uuid+"' for details.")
	fmt.Fprintln(w, res.Stdout)
}

func SummarizeSimple(w io.Writer, uuid, summary string) {
	fmt.Fprintf(w, "Session created: %s\n", uuid)
	fmt.Fprintln(w, summary)
}
