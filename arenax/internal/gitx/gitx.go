package gitx

import (
	"bytes"
	"errors"
	"os/exec"
	"strings"
)

// Common git errors.
var (
	ErrNotARepo = errors.New("not a git repository")
	ErrGit      = errors.New("git command failed")
)

// StagedDiff returns the output of `git diff --cached` (staged changes only).
// repo is the path to the working tree ("" or "." means current directory).
// The function is pure with respect to the immutable Git object store:
// identical repo state always yields identical diff bytes.
// Usage example:
//
//	diff, err := gitx.StagedDiff(".")
//	if err != nil { ... }
//	// diff is the exact patch text for sizebound + arena --context @file
func StagedDiff(repo string) (string, error) {
	return runGitDiff(repo, "--cached")
}

// RangeDiff returns `git diff <a>..<b>` for an arbitrary commit range.
// a and b may be any revs git accepts (tags, SHAs, refs).
// Usage:
//
//	diff, err := gitx.RangeDiff(".", "main", "HEAD")
func RangeDiff(repo, a, b string) (string, error) {
	if a == "" || b == "" {
		return "", errors.New("range requires non-empty a and b")
	}
	return runGitDiff(repo, a+".."+b)
}

// ChangedFiles returns the list of paths changed in working tree vs HEAD
// (equivalent to `git diff --name-only HEAD`). Includes untracked? No:
// this is for drift which uses working vs HEAD per DSN.
func ChangedFiles(repo string) ([]string, error) {
	args := []string{"diff", "--name-only", "HEAD"}
	if repo != "" && repo != "." {
		args = append([]string{"-C", repo}, args...)
	} else {
		args = append([]string{"-C", "."}, args...)
	}
	cmd := exec.Command("git", args...)
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		if bytes.Contains(stderr.Bytes(), []byte("not a git repository")) || bytes.Contains(stderr.Bytes(), []byte("unknown option `cached'")) {
			return nil, ErrNotARepo
		}
		return nil, errors.Join(ErrGit, err)
	}
	lines := strings.Split(strings.TrimSpace(stdout.String()), "\n")
	if len(lines) == 1 && lines[0] == "" {
		return []string{}, nil
	}
	// filter empties
	out := make([]string, 0, len(lines))
	for _, l := range lines {
		if l != "" {
			out = append(out, l)
		}
	}
	return out, nil
}

func runGitDiff(repo string, spec string) (string, error) {
	args := []string{"diff", spec, "--no-color", "--no-ext-diff", "--no-textconv"}
	if repo != "" && repo != "." {
		args = append([]string{"-C", repo}, args...)
	} else {
		args = append([]string{"-C", "."}, args...)
	}
	cmd := exec.Command("git", args...)
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		if bytes.Contains(stderr.Bytes(), []byte("not a git repository")) || bytes.Contains(stderr.Bytes(), []byte("unknown option `cached'")) {
			return "", ErrNotARepo
		}
		// git diff on no changes exits 0 with empty; non-zero only for real errors
		return "", errors.Join(ErrGit, err)
	}
	return stdout.String(), nil
}
