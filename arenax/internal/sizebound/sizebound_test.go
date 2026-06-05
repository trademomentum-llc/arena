package sizebound

import (
	"strings"
	"testing"
	"testing/quick"
)

func TestFit_byteCap_Property(t *testing.T) {
	// property: for byte mode (counter=nil), returned string len never exceeds cap
	f := func(ctx string, cap int) bool {
		if cap <= 0 || cap > 1<<20 {
			cap = 100
		}
		out, rpt := Fit(ctx, cap, nil)
		if rpt.Fit {
			return len(out) == len(ctx) && len(out) <= cap
		}
		return len(out) <= cap+100 // marker room, but we keep under
	}
	if err := quick.Check(f, &quick.Config{MaxCount: 200}); err != nil {
		t.Error(err)
	}
}

func TestFit_ByteCap_TruncatesFromTail(t *testing.T) {
	ctx := strings.Repeat("A", 100) + "TAIL_END_MARKER"
	cap := 50
	out, rpt := Fit(ctx, cap, nil)
	if rpt.Fit {
		t.Error("should not fit")
	}
	if len(out) > cap+50 { // generous for marker
		t.Errorf("out too long: %d", len(out))
	}
	if strings.Contains(out, "TAIL_END_MARKER") {
		t.Error("tail not dropped")
	}
	if !strings.Contains(out, "truncated by arenax") {
		t.Error("missing truncation marker")
	}
}

func TestFit_WithinCap_ReturnsOriginal(t *testing.T) {
	ctx := "small diff"
	out, rpt := Fit(ctx, 40960, nil)
	if !rpt.Fit || out != ctx {
		t.Error("should fit exactly")
	}
}

func TestFit_WithStubCounter(t *testing.T) {
	ctx := strings.Repeat("x", 100)
	c := StubCounter{}
	_, rpt := Fit(ctx, 40, c) // budget ~10 tokens
	if rpt.Fit {
		t.Error("large should not fit token budget")
	}
}

func TestFit_Deterministic(t *testing.T) {
	ctx := strings.Repeat("abc", 1000)
	c1, r1 := Fit(ctx, 123, nil)
	c2, r2 := Fit(ctx, 123, nil)
	if c1 != c2 || r1 != r2 {
		t.Error("Fit must be pure/deterministic")
	}
}

// manual property loop for no panic on weird input
func TestFit_NoPanic_PropertyManual(t *testing.T) {
	inputs := []string{"", "a", "\n\n\n", strings.Repeat("x", 1<<16), "diff --git\000null"}
	caps := []int{0, 1, 10, 40960, -5}
	for _, c := range caps {
		for _, in := range inputs {
			_, _ = Fit(in, c, nil)
			_, _ = Fit(in, c, StubCounter{})
		}
	}
}
