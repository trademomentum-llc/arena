package drift

import (
	"os"
	"path/filepath"
	"reflect"
	"sort"
	"strings"
	"testing"
)

func TestClassify_table(t *testing.T) {
	cases := []struct {
		name  string
		paths []string
		specs []string
		impls []string
	}{
		{
			"mixed typical",
			[]string{"README.md", "docs/API.md", "src/main.go", "lib/foo.rs", "internal/bar.py", "tests/test.js", "foo.txt"},
			[]string{"README.md", "docs/API.md"},
			[]string{"src/main.go", "lib/foo.rs", "internal/bar.py"},
		},
		{
			"specs dir with json",
			[]string{"design/specs/thing.json", "specs/req.yaml", "src/impl.go"},
			[]string{"design/specs/thing.json", "specs/req.yaml"},
			[]string{"src/impl.go"},
		},
		{
			"under specs anything? but per rules md etc",
			[]string{"specs/design.md", "specs/code.go"}, // code.go under specs -> spec? rules say or ext under specs dir -> yes spec for md; for .go ?
			[]string{"specs/design.md", "specs/code.go"}, // conservative: if under specs/ treat as spec even .go per broad "under a specs dir"
			[]string{},
		},
		{
			"top level spec docs",
			[]string{"spec/ADR-001.md", "CHANGELOG.md", "src/lib.c"},
			[]string{"spec/ADR-001.md"},
			[]string{"src/lib.c"},
		},
		{
			"ignored",
			[]string{"vendor/dep.go", "node_modules/pkg.js", ".github/workflow.yml", "random.bin"},
			[]string{},
			[]string{},
		},
		{
			"impl by ext anywhere",
			[]string{"some/deep/thing.ts", "root.rs"},
			[]string{},
			[]string{"some/deep/thing.ts", "root.rs"},
		},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			gotS, gotI := Classify(c.paths)
			sort.Strings(gotS)
			sort.Strings(gotI)
			sort.Strings(c.specs)
			sort.Strings(c.impls)
			if !reflect.DeepEqual(gotS, c.specs) {
				t.Errorf("specs: got %v want %v", gotS, c.specs)
			}
			if !reflect.DeepEqual(gotI, c.impls) {
				t.Errorf("impls: got %v want %v", gotI, c.impls)
			}
		})
	}
}

func TestExpandToFiles(t *testing.T) {
	tmp := t.TempDir()
	// create tree
	os.MkdirAll(filepath.Join(tmp, "d1/d2"), 0755)
	write(t, filepath.Join(tmp, "f1.go"), "x")
	write(t, filepath.Join(tmp, "d1/f2.rs"), "y")
	write(t, filepath.Join(tmp, "d1/d2/f3.py"), "z")
	// symlink skipped
	os.Symlink(filepath.Join(tmp, "f1.go"), filepath.Join(tmp, "link.go"))

	files, err := ExpandToFiles([]string{tmp, "nonexistent"})
	if err != nil {
		t.Fatal(err)
	}
	if len(files) != 3 {
		t.Fatalf("expected 3 regular files, got %d: %v", len(files), files)
	}
	// ensure no dirs, no link
	for _, f := range files {
		if strings.Contains(f, "link") || isDir(t, f) {
			t.Errorf("bad file in result: %s", f)
		}
	}
}

func TestClassify_Empty(t *testing.T) {
	s, i := Classify(nil)
	if len(s)+len(i) != 0 {
		t.Error("empty input")
	}
}

func TestExpand_IgnoresNonRegular(t *testing.T) {
	tmp := t.TempDir()
	// fifo or just rely on dir+file
	write(t, filepath.Join(tmp, "only.go"), "")
	files, _ := ExpandToFiles([]string{tmp})
	if len(files) != 1 {
		t.Error("expected the file")
	}
}

func write(t *testing.T, p, c string) {
	t.Helper()
	if err := os.WriteFile(p, []byte(c), 0644); err != nil {
		t.Fatal(err)
	}
}

func isDir(t *testing.T, p string) bool {
	t.Helper()
	fi, _ := os.Stat(p)
	return fi != nil && fi.IsDir()
}
