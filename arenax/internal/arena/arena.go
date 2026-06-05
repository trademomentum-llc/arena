package arena

import (
	"bytes"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"syscall"
)

// SessionSpec describes the parameters for arena create.
type SessionSpec struct {
	Type    string
	Task    string
	Context string   // diff text; will be written @tempfile
	Workers []string // e.g. []string{"qwen-coder-local"}
	Mode    string   // "human-in-loop" or "council"
}

// Result captures the outcome of a subcommand invocation.
type Result struct {
	Stdout   string
	Stderr   string
	ExitCode int
}

// DriftResult is the parsed outcome of drift-check.
type DriftResult struct {
	Stdout   string
	Findings int // number of findings lines; 0 means "No drift detected..."
	ExitCode int
}

// Client is the typed wrapper over the arena CLI contract.
// Bin may be "arena" (looked up) or absolute path.
// Env is the environment passed to the child (includes caller env + overrides).
// Secrets (API keys) MUST only appear in Env, never in any argv element.
type Client struct {
	Bin string
	Env []string
}

// Err* are the typed sentinel errors per DSN 7.
var (
	ErrArenaMissing  = errors.New("arena binary not found on PATH")
	ErrEndpointDown  = errors.New("local endpoint unreachable")
	ErrNoChanges     = errors.New("no changes to review")
	ErrSessionParse  = errors.New("failed to parse session id from arena output")
	ErrArenaNonZero  = errors.New("arena exited non-zero")
	ErrNoImpls       = errors.New("no implementation files after classification")
)

// ExtractUUID parses the literal marker from arena create (or finalize) stdout.
// It matches lines starting exactly with the documented prefix and validates
// as RFC4122-ish UUID. This is the single exported entry for UUID parsing.
// Must match the contract in arena/src/cli/commands.rs .
//
// Usage example:
//
//	uuid, err := arena.ExtractUUID("... \nSession created: 123e4567-e89b-12d3-a456-426614174000\nUse ...")
func ExtractUUID(stdout string) (string, error) {
	prefix := "Session created: "
	for _, line := range strings.Split(stdout, "\n") {
		line = strings.TrimSpace(line)
		if strings.HasPrefix(line, prefix) {
			cand := strings.TrimSpace(line[len(prefix):])
			if isRFC4122(cand) {
				return cand, nil
			}
			// also support finalize marker for symmetry in tests
			// but primary is created
		}
		if strings.HasPrefix(line, "Session finalized: ") {
			cand := strings.TrimSpace(line[len("Session finalized: "):])
			if isRFC4122(cand) {
				return cand, nil
			}
		}
	}
	return "", fmt.Errorf("%w: no valid UUID after marker in stdout", ErrSessionParse)
}

var uuidRe = regexp.MustCompile(`^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$`)

func isRFC4122(s string) bool {
	return uuidRe.MatchString(s)
}

// NewClient constructs a Client. If bin == "", looks up "arena".
// Env if nil uses os.Environ().
func NewClient(bin string, env []string) (Client, error) {
	if bin == "" {
		bin = "arena"
	}
	if !filepath.IsAbs(bin) {
		p, err := exec.LookPath(bin)
		if err != nil {
			return Client{}, fmt.Errorf("%w: %s", ErrArenaMissing, bin)
		}
		bin = p
	}
	if env == nil {
		env = os.Environ()
	}
	return Client{Bin: bin, Env: env}, nil
}

// run executes the arena subcommand with given args. Never puts secret values
// from env into args (validated by caller tests).
func (c Client) run(args []string) (Result, error) {
	// ensure bin resolved
	if c.Bin == "" {
		return Result{ExitCode: 127}, ErrArenaMissing
	}
	if _, statErr := os.Stat(c.Bin); statErr != nil {
		return Result{ExitCode: 127}, fmt.Errorf("%w: %s", ErrArenaMissing, c.Bin)
	}
	cmd := exec.Command(c.Bin, args...)
	cmd.Env = c.Env
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	err := cmd.Run()
	rc := 0
	if err != nil {
		if exitErr, ok := err.(*exec.ExitError); ok {
			if status, ok := exitErr.Sys().(syscall.WaitStatus); ok {
				rc = status.ExitStatus()
			} else {
				rc = 1
			}
		} else {
			rc = 1
		}
	}
	return Result{Stdout: stdout.String(), Stderr: stderr.String(), ExitCode: rc}, nil
}

// Create invokes `arena create ...` and returns the extracted session UUID.
// Context is ALWAYS written to a 0600 temp file and passed as @path (FR-11).
// The temp is removed after the arena process exits (success or fail).
// Task/Workers etc are assembled deterministically.
func (c Client) Create(spec SessionSpec) (string, error) {
	if spec.Context == "" {
		return "", ErrNoChanges
	}
	// write temp 0600
	tmpf, err := os.CreateTemp("", "arenax-ctx-*.txt")
	if err != nil {
		return "", err
	}
	tmpPath := tmpf.Name()
	// ensure 0600 (CreateTemp already does, but be explicit)
	if err := tmpf.Chmod(0600); err != nil {
		tmpf.Close()
		os.Remove(tmpPath)
		return "", err
	}
	if _, err := tmpf.WriteString(spec.Context); err != nil {
		tmpf.Close()
		os.Remove(tmpPath)
		return "", err
	}
	tmpf.Close()
	defer os.Remove(tmpPath) // hygiene, NFR-5

	args := BuildArgvForCreate(spec, "@"+tmpPath)
	// sanity: no secret-looking thing in args (keys are in env only).
	// We skip the @path because it is our controlled temp, not a secret value.
	for _, a := range args {
		if !strings.HasPrefix(a, "@") && looksLikeSecret(a) {
			os.Remove(tmpPath)
			return "", fmt.Errorf("refusing to put secret-like value in argv")
		}
	}

	res, err := c.run(args)
	if err != nil {
		return "", err
	}
	if res.ExitCode != 0 {
		return "", fmt.Errorf("%w: exit=%d stderr=%s", ErrArenaNonZero, res.ExitCode, res.Stderr)
	}
	uuid, err := ExtractUUID(res.Stdout)
	if err != nil {
		return "", fmt.Errorf("create stdout: %s\n%w", res.Stdout, err)
	}
	return uuid, nil
}

// Run invokes `arena run --session-id <uuid>` and returns captured output.
func (c Client) Run(uuid string) (Result, error) {
	if uuid == "" {
		return Result{}, errors.New("empty uuid")
	}
	args := []string{"run", "--session-id", uuid}
	res, err := c.run(args)
	if err != nil {
		return res, err
	}
	if res.ExitCode != 0 {
		return res, fmt.Errorf("%w: run exit %d: %s", ErrArenaNonZero, res.ExitCode, res.Stderr)
	}
	return res, nil
}

// DriftCheck invokes `arena drift-check --specs a,b --impls c,d --agent X`
func (c Client) DriftCheck(specs, impls []string, agent string) (DriftResult, error) {
	if len(impls) == 0 {
		return DriftResult{ExitCode: 5}, ErrNoImpls
	}
	args := []string{
		"drift-check",
		"--specs", strings.Join(specs, ","),
		"--impls", strings.Join(impls, ","),
		"--agent", agent,
	}
	res, err := c.run(args)
	if err != nil {
		return DriftResult{}, err
	}
	dr := DriftResult{Stdout: res.Stdout, ExitCode: res.ExitCode}
	if strings.Contains(res.Stdout, "No drift detected") {
		dr.Findings = 0
	} else if strings.Contains(res.Stdout, "Drift findings (") {
		// crude count; real parser could split
		dr.Findings = strings.Count(res.Stdout, "\n  [")
	}
	if res.ExitCode != 0 {
		return dr, fmt.Errorf("%w: drift-check exit %d", ErrArenaNonZero, res.ExitCode)
	}
	return dr, nil
}

// BuildArgvForCreate is a pure function that assembles the argv for a create
// invocation. It is exported for property testing (never put secrets in argv).
// The contextPath must already be the @/path form or literal.
func BuildArgvForCreate(spec SessionSpec, contextPath string) []string {
	workersStr := strings.Join(spec.Workers, ",")
	return []string{
		"create",
		"--session-type", spec.Type,
		"--mode", spec.Mode,
		"--workers", workersStr,
		"--task", spec.Task,
		"-x", contextPath,
	}
}

// looksLikeSecret is a heuristic used in tests and Create to ensure
// we never leak env values that look like keys into argv.
func looksLikeSecret(s string) bool {
	s = strings.ToLower(s)
	return strings.Contains(s, "key") || strings.Contains(s, "token") || strings.Contains(s, "secret") || len(s) > 40 && strings.ContainsAny(s, "_-.")
}
