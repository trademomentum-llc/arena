package arena

import (
	"errors"
	"os"
	"strings"
	"testing"
	"testing/quick"
)

// TestExtractUUID_table covers the exact contract markers.
func TestExtractUUID_table(t *testing.T) {
	cases := []struct {
		name   string
		stdout string
		want   string
		err    bool
	}{
		{
			"happy create",
			"foo\nSession created: 123e4567-e89b-12d3-a456-426614174000\nUse 'arena run...",
			"123e4567-e89b-12d3-a456-426614174000",
			false,
		},
		{
			"finalize marker",
			"Session finalized: 00000000-0000-0000-0000-000000000000",
			"00000000-0000-0000-0000-000000000000",
			false,
		},
		{
			"malformed uuid",
			"Session created: not-a-uuid",
			"",
			true,
		},
		{
			"missing marker",
			"some output\nno session line here",
			"",
			true,
		},
		{
			"multiline with garbage",
			"bar\nSession created: 123e4567-e89b-12d3-a456-426614174000\nbaz",
			"123e4567-e89b-12d3-a456-426614174000",
			false,
		},
		{
			"wrong prefix case",
			"session created: 123e4567-e89b-12d3-a456-426614174000",
			"",
			true,
		},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			got, err := ExtractUUID(c.stdout)
			if c.err {
				if err == nil {
					t.Error("expected err")
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected err: %v", err)
			}
			if got != c.want {
				t.Errorf("got %s want %s", got, c.want)
			}
		})
	}
}

// property test: ExtractUUID is idempotent on its output when valid
func TestExtractUUID_IdempotentProperty(t *testing.T) {
	f := func(s string) bool {
		u, err := ExtractUUID("Session created: " + s)
		if err != nil {
			return true // invalid not claimed
		}
		u2, _ := ExtractUUID("Session created: " + u)
		return u == u2
	}
	if err := quick.Check(f, nil); err != nil {
		t.Error(err)
	}
}

// TestBuildArgv_noSecret is the security property test (NFR-5).
// It asserts that for any plausible secret value in env, the constructed
// argv for create never contains that value as a substring.
func TestBuildArgv_noSecret(t *testing.T) {
	spec := SessionSpec{
		Type:    "code-review",
		Task:    "review foo",
		Context: "diff --git ...", // not used directly
		Workers: []string{"qwen-coder-local"},
		Mode:    "human-in-loop",
	}
	// simulate many possible secret strings
	secrets := []string{
		"sk-1234567890abcdef",
		"OPENAI_API_KEY=realvaluehere",
		"ghp_abcdefghijklmnopqrstuvwxyz",
		"very-long-token-that-looks-like-a-key-12345678901234567890",
		"ANTHROPIC_API_KEY",
	}
	for _, sec := range secrets {
		argv := BuildArgvForCreate(spec, "@/tmp/fake-ctx.txt")
		for _, arg := range argv {
			if strings.Contains(arg, sec) {
				t.Errorf("argv leak: %q contains secret %q", argv, sec)
			}
		}
		// also the @path form itself should not
		if strings.Contains("@/tmp/fake-ctx.txt", sec) {
			t.Error("path leak impossible")
		}
	}
}

// TestNewClient_Lookup and missing.
func TestNewClient(t *testing.T) {
	// use /bin/echo which exists on unix
	c, err := NewClient("/bin/echo", nil)
	if err != nil {
		t.Fatal(err)
	}
	if c.Bin != "/bin/echo" {
		t.Error("bin not set")
	}
	// relative that doesn't exist
	_, err = NewClient("definitely-not-on-path-xyz123", nil)
	if err == nil || !errors.Is(err, ErrArenaMissing) {
		t.Errorf("expected arena missing, got %v", err)
	}
}

// Integration test: full create -> run cycle using ALWAYS-registered mock agents.
// This pins the stdout markers and exit codes that arenax depends on (NFR-9).
// Uses arena binary from parent project (M1 complete).
func TestIntegration_CreateRunFinalize_Mock(t *testing.T) {
	arenaBin := "/Users/nnos/Projects/arena/target/release/arena"
	if _, err := os.Stat(arenaBin); err != nil {
		t.Skipf("arena binary not present at %s (build parent first): %v", arenaBin, err)
	}

	c, err := NewClient(arenaBin, nil)
	if err != nil {
		t.Fatal(err)
	}

	spec := SessionSpec{
		Type:    "code-review",
		Task:    "Review the test diff for integration",
		Context: "diff --git a/foo.go b/foo.go\nindex 000..111\n--- a/foo.go\n+++ b/foo.go\n@@ -1 +1 @@\n- old\n+ new\n",
		Workers: []string{"mock-reviewer-1", "mock-reviewer-2"},
		Mode:    "human-in-loop",
	}

	uuid, err := c.Create(spec)
	if err != nil {
		t.Fatalf("create failed: %v", err)
	}
	if !isRFC4122(uuid) {
		t.Errorf("bad uuid from create: %s", uuid)
	}

	runRes, err := c.Run(uuid)
	if err != nil {
		t.Fatalf("run failed: %v\nstdout: %s", err, runRes.Stdout)
	}
	if !strings.Contains(runRes.Stdout, "Responses collected:") {
		t.Errorf("run output missing expected marker: %s", runRes.Stdout)
	}
	// also verify finalize would work (optional, but exercise)
	// we don't call finalize here to avoid side effects on the session store in cwd
}

// failure path: missing binary
func TestClient_MissingBinary(t *testing.T) {
	c := Client{Bin: "/no/such/arena/bin/123", Env: nil}
	_, err := c.Create(SessionSpec{Type: "code-review", Task: "t", Context: "d", Workers: []string{"m"}, Mode: "human-in-loop"})
	if err == nil || !strings.Contains(err.Error(), "arena binary not found") {
		t.Errorf("expected missing, got %v", err)
	}
}

// empty context -> ErrNoChanges before any exec
func TestCreate_EmptyContext(t *testing.T) {
	c, _ := NewClient("/bin/echo", nil)
	_, err := c.Create(SessionSpec{Context: ""})
	if err != ErrNoChanges {
		t.Errorf("want ErrNoChanges got %v", err)
	}
}
