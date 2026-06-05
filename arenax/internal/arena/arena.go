// internal/arena/arena.go - thin typed client over arena CLI contract (NFR-9)
package arena

import (
	"bytes"
	"fmt"
	"os"
	"os/exec"
	"strings"
)

type Client struct {
	Bin string
	Env []string
}

type SessionSpec struct {
	Type    string
	Task    string
	Context string // will be written to @temp if large
	Workers []string
	Mode    string
}

type Result struct {
	Stdout   string
	ExitCode int
}

type DriftFinding struct {
	Severity    string
	Description string
}

type DriftResult struct {
	Findings []DriftFinding
	Raw      string
}

// Create runs "arena create ..." and returns the UUID from stdout.
func (c Client) Create(spec SessionSpec) (string, error) {
	args := []string{
		"create",
		"--session-type", spec.Type,
		"--task", spec.Task,
		"--workers", strings.Join(spec.Workers, ","),
		"--mode", spec.Mode,
	}
	if spec.Context != "" {
		// Write to secure temp and pass @path (FR-11, NFR-5)
		tmp, err := os.CreateTemp("", "arenax-ctx-*.diff")
		if err != nil {
			return "", err
		}
		defer os.Remove(tmp.Name()) // best effort; arena will have read it
		if err := os.Chmod(tmp.Name(), 0600); err != nil {
			tmp.Close()
			return "", err
		}
		if _, err := tmp.WriteString(spec.Context); err != nil {
			tmp.Close()
			return "", err
		}
		tmp.Close()
		args = append(args, "-x", "@"+tmp.Name())
	}

	cmd := c.buildCmd(args...)
	stdout, stderr, err := runCapture(cmd)
	if err != nil {
		return "", fmt.Errorf("arena create: %v\n%s", err, stderr)
	}
	uuid, err := ExtractUUID(stdout)
	if err != nil {
		return "", fmt.Errorf("parse create stdout: %w\nfull: %s", err, stdout)
	}
	return uuid, nil
}

func (c Client) Run(uuid string) (Result, error) {
	cmd := c.buildCmd("run", "--session-id", uuid)
	stdout, stderr, err := runCapture(cmd)
	if err != nil {
		return Result{Stdout: stdout + "\n" + stderr, ExitCode: 1}, err
	}
	return Result{Stdout: stdout}, nil
}

func (c Client) DriftCheck(specs, impls []string, agent string) (DriftResult, error) {
	args := []string{"drift-check", "--specs", strings.Join(specs, ","), "--impls", strings.Join(impls, ","), "--agent", agent}
	cmd := c.buildCmd(args...)
	stdout, _, err := runCapture(cmd)
	// arena drift-check may exit non-zero? but we parse output
	res := DriftResult{Raw: stdout}
	if strings.Contains(stdout, "No drift detected") {
		return res, nil
	}
	// naive parse for findings (improve with real parser)
	lines := strings.Split(stdout, "\n")
	for _, l := range lines {
		l = strings.TrimSpace(l)
		if strings.HasPrefix(l, "[") {
			// simplistic
			res.Findings = append(res.Findings, DriftFinding{Severity: "info", Description: l})
		}
	}
	return res, err
}

func (c Client) buildCmd(args ...string) *exec.Cmd {
	cmd := exec.Command(c.Bin, args...)
	cmd.Env = c.Env
	if cmd.Env == nil {
		cmd.Env = os.Environ()
	}
	return cmd
}

func runCapture(cmd *exec.Cmd) (stdout, stderr string, err error) {
	var so, se bytes.Buffer
	cmd.Stdout = &so
	cmd.Stderr = &se
	err = cmd.Run()
	return so.String(), se.String(), err
}

// ExtractUUID parses the exact marker from arena create output.
func ExtractUUID(stdout string) (string, error) {
	for _, line := range strings.Split(stdout, "\n") {
		line = strings.TrimSpace(line)
		if strings.HasPrefix(line, "Session created: ") {
			cand := strings.TrimSpace(line[len("Session created: "):])
			if isRFC4122(cand) {
				return cand, nil
			}
			return "", fmt.Errorf("invalid UUID after marker: %s", cand)
		}
	}
	return "", fmt.Errorf("no 'Session created: <UUID>' marker found")
}

func isRFC4122(s string) bool {
	// 8-4-4-4-12 hex with hyphens
	parts := strings.Split(s, "-")
	if len(parts) != 5 {
		return false
	}
	return len(parts[0]) == 8 && len(parts[1]) == 4 && len(parts[2]) == 4 && len(parts[3]) == 4 && len(parts[4]) == 12
}
