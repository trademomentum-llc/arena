package gitx

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func TestStagedDiff_EmptyRepo(t *testing.T) {
	tmp := t.TempDir()
	runGit(t, tmp, "init")
	runGit(t, tmp, "config", "user.email", "t@t")
	runGit(t, tmp, "config", "user.name", "t")

	// no staged -> empty
	d, err := StagedDiff(tmp)
	if err != nil {
		t.Fatalf("StagedDiff on clean: %v", err)
	}
	if d != "" {
		t.Errorf("expected empty diff, got %q", d)
	}
}

func TestStagedDiff_WithChange(t *testing.T) {
	tmp := t.TempDir()
	runGit(t, tmp, "init")
	runGit(t, tmp, "config", "user.email", "t@t")
	runGit(t, tmp, "config", "user.name", "t")
	writeFile(t, tmp, "f.txt", "hello\n")
	runGit(t, tmp, "add", "f.txt")
	runGit(t, tmp, "commit", "-m", "init")

	writeFile(t, tmp, "f.txt", "hello\nworld\n")
	runGit(t, tmp, "add", "f.txt")

	d, err := StagedDiff(tmp)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(d, "+world") {
		t.Errorf("diff missing change: %s", d)
	}
}

func TestRangeDiff(t *testing.T) {
	tmp := t.TempDir()
	runGit(t, tmp, "init")
	runGit(t, tmp, "config", "user.email", "t@t")
	runGit(t, tmp, "config", "user.name", "t")
	writeFile(t, tmp, "a.txt", "1\n")
	runGit(t, tmp, "add", "a.txt")
	runGit(t, tmp, "commit", "-m", "c1")
	writeFile(t, tmp, "a.txt", "1\n2\n")
	runGit(t, tmp, "add", "a.txt")
	runGit(t, tmp, "commit", "-m", "c2")

	d, err := RangeDiff(tmp, "HEAD~1", "HEAD")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(d, "+2") {
		t.Error("range diff wrong")
	}
}

func TestChangedFiles(t *testing.T) {
	tmp := t.TempDir()
	runGit(t, tmp, "init")
	runGit(t, tmp, "config", "user.email", "t@t")
	runGit(t, tmp, "config", "user.name", "t")
	writeFile(t, tmp, "x.go", "package x\n")
	runGit(t, tmp, "add", "x.go")
	runGit(t, tmp, "commit", "-m", "x")
	writeFile(t, tmp, "y.md", "spec\n")
	// y untracked, x unchanged -> should be empty vs HEAD
	fs, err := ChangedFiles(tmp)
	if err != nil {
		t.Fatal(err)
	}
	if len(fs) != 0 {
		t.Errorf("expected no changes vs HEAD, got %v", fs)
	}
	// modify x
	writeFile(t, tmp, "x.go", "package x\n// mod\n")
	fs, err = ChangedFiles(tmp)
	if err != nil {
		t.Fatal(err)
	}
	if len(fs) != 1 || fs[0] != "x.go" {
		t.Errorf("got %v", fs)
	}
}

func TestNotRepo(t *testing.T) {
	tmp := t.TempDir()
	_, err := StagedDiff(tmp)
	if err == nil || !strings.Contains(err.Error(), "not a git") {
		t.Errorf("expected ErrNotARepo, got %v", err)
	}
}

func TestRangeRequiresArgs(t *testing.T) {
	_, err := RangeDiff(".", "", "HEAD")
	if err == nil {
		t.Error("expected error for empty range rev")
	}
}

// property-ish: calling twice with no intervening git yields identical output
func TestStagedDiff_Idempotent(t *testing.T) {
	tmp := t.TempDir()
	runGit(t, tmp, "init")
	runGit(t, tmp, "config", "user.email", "t@t")
	runGit(t, tmp, "config", "user.name", "t")
	writeFile(t, tmp, "p.txt", "base\n")
	runGit(t, tmp, "add", "p.txt")
	runGit(t, tmp, "commit", "-m", "b")
	writeFile(t, tmp, "p.txt", "base\nmod\n")
	runGit(t, tmp, "add", "p.txt")

	d1, _ := StagedDiff(tmp)
	d2, _ := StagedDiff(tmp)
	if d1 != d2 {
		t.Error("StagedDiff not idempotent for same state")
	}
}

func runGit(t *testing.T, dir string, args ...string) {
	t.Helper()
	cmd := exec.Command("git", args...)
	cmd.Dir = dir
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("git %v failed: %v\n%s", args, err, out)
	}
}

func writeFile(t *testing.T, dir, name, content string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(dir, name), []byte(content), 0644); err != nil {
		t.Fatal(err)
	}
}
