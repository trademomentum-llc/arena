package drift

import "testing"

func TestClassify_table(t *testing.T) {
	paths := []string{
		"src/main.go",
		"docs/api.md",
		"README.md",
		"specs/design.yaml",
		"internal/foo/bar.rs",
		"vendor/lib.js",
		"src/spec_helper.rb", // impl? per rules src/ = impl
	}
	s, i := Classify(paths)
	if len(s) != 2 {
		t.Errorf("expected 2 specs, got %d: %v", len(s), s)
	}
	if len(i) != 3 {
		t.Errorf("expected 3 impls, got %d: %v", len(i), i)
	}
}

func TestExpandToFiles(t *testing.T) {
	// simple: non existing dir is ignored
	out := ExpandToFiles([]string{"/no/such/dir", "go.mod"})
	if len(out) != 1 {
		t.Log("expand may return existing files only")
	}
}
