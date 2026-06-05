// internal/gitx/gitx.go - pure git read-only wrappers per spec
package gitx

import (
	"bytes"
	"os/exec"
	"strings"
)

// StagedDiff returns git diff --cached for the repo dir.
func StagedDiff(repo string) (string, error) {
	return runGit(repo, "diff", "--cached")
}

// RangeDiff returns git diff a..b .
func RangeDiff(repo, a, b string) (string, error) {
	return runGit(repo, "diff", a+".."+b)
}

// ChangedFiles returns git diff --name-only HEAD .
func ChangedFiles(repo string) ([]string, error) {
	out, err := runGit(repo, "diff", "--name-only", "HEAD")
	if err != nil {
		return nil, err
	}
	if strings.TrimSpace(out) == "" {
		return []string{}, nil
	}
	lines := strings.Split(strings.TrimSpace(out), "\n")
	return lines, nil
}

func runGit(dir string, args ...string) (string, error) {
	cmd := exec.Command("git", args...)
	cmd.Dir = dir
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		return "", err
	}
	return stdout.String(), nil
}
