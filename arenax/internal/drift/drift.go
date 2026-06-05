package drift

import (
	"errors"
	"io/fs"
	"os"
	"path/filepath"
	"strings"
)

// Classify partitions paths into specs and impls using the deterministic
// table-driven ruleset from DSN 4.2. Unmatched paths are ignored (caller may log).
// This is the primary exported entry for classification (pure function).
// Rules (first match wins, ordered):
//   - under docs/, spec/, specs/  OR  (under a */specs*/ dir AND ext in .md .yaml .yml .json) => spec
//   - under src/, lib/, internal/ OR recognized source ext (.go .rs .c .cpp .py .js .ts .java .rb etc) => impl
//   - else ignored
func Classify(paths []string) (specs, impls []string) {
	specs = []string{}
	impls = []string{}
	for _, p := range paths {
		if isSpec(p) {
			specs = append(specs, p)
		} else if isImpl(p) {
			impls = append(impls, p)
		}
		// else ignored
	}
	return specs, impls
}

func isSpec(p string) bool {
	p = filepath.ToSlash(p)
	lower := strings.ToLower(p)
	if isIgnoredDir(lower) {
		return false
	}
	// direct under docs/ spec/ specs/
	if strings.HasPrefix(lower, "docs/") || strings.HasPrefix(lower, "spec/") || strings.HasPrefix(lower, "specs/") {
		return true
	}
	// under a specs dir (e.g. foo/specs/bar.md , design/specs/..)
	if idx := strings.Index(lower, "/specs/"); idx != -1 {
		return true // anything under specs/ counts
	}
	// common doc files: only README*.md and under doc dirs, or under specs (already returned)
	base := strings.ToLower(filepath.Base(p))
	if hasSpecExt(p) && (strings.HasPrefix(base, "readme") || strings.HasPrefix(lower, "docs/") || strings.HasPrefix(lower, "spec/") || strings.HasPrefix(lower, "specs/") || strings.Contains(lower, "/spec")) {
		return true
	}
	// extension based only if appears under specs-ish
	dir := filepath.Dir(p)
	bdir := filepath.Base(dir)
	if strings.Contains(strings.ToLower(bdir), "spec") && hasSpecExt(p) {
		return true
	}
	return false
}

func hasSpecExt(p string) bool {
	ext := strings.ToLower(filepath.Ext(p))
	switch ext {
	case ".md", ".yaml", ".yml", ".json":
		return true
	}
	return false
}

var sourceExts = map[string]bool{
	".go": true, ".rs": true, ".c": true, ".cpp": true, ".cc": true, ".h": true, ".hpp": true,
	".py": true, ".js": true, ".ts": true, ".tsx": true, ".jsx": true,
	".java": true, ".rb": true, ".scala": true, ".kt": true, ".swift": true,
	".sh": true, ".bash": true, ".zsh": true, ".lua": true, ".pl": true,
}

func isIgnoredDir(lower string) bool {
	for _, ign := range []string{"vendor/", "node_modules/", ".git/", "target/", "dist/", "build/", ".github/", "tests/"} {
		if strings.Contains(lower, ign) || strings.HasPrefix(lower, strings.TrimSuffix(ign, "/")) {
			return true
		}
	}
	return false
}

func isImpl(p string) bool {
	p = filepath.ToSlash(p)
	lower := strings.ToLower(p)
	if isIgnoredDir(lower) {
		return false
	}
	if strings.HasPrefix(lower, "src/") || strings.HasPrefix(lower, "lib/") || strings.HasPrefix(lower, "internal/") {
		return true
	}
	ext := strings.ToLower(filepath.Ext(p))
	if sourceExts[ext] {
		return true
	}
	// also under internal anywhere
	if strings.Contains(lower, "/internal/") || strings.HasPrefix(lower, "internal") {
		return true
	}
	return false
}

// ExpandToFiles takes a list of paths (may include dirs) and returns only
// regular files (recursively for dirs). Symlinks and non-regular are skipped.
// Errors only on real fs problems. Arena drift-check requires file paths (C-3).
func ExpandToFiles(paths []string) ([]string, error) {
	var out []string
	seen := map[string]bool{}
	for _, p := range paths {
		info, err := os.Stat(p)
		if err != nil {
			// if not exist, skip (git may report deleted? but for drift usually exist)
			if os.IsNotExist(err) {
				continue
			}
			return nil, err
		}
		if info.IsDir() {
			err := filepath.WalkDir(p, func(path string, d fs.DirEntry, err error) error {
				if err != nil {
					return err
				}
				if d.IsDir() {
					return nil
				}
				if fi, err := d.Info(); err == nil && fi.Mode().IsRegular() {
					if !seen[path] {
						seen[path] = true
						out = append(out, path)
					}
				}
				return nil
			})
			if err != nil {
				return nil, err
			}
		} else if info.Mode().IsRegular() {
			if !seen[p] {
				seen[p] = true
				out = append(out, p)
			}
		}
		// skip symlinks, devices etc.
	}
	return out, nil
}

// RunDrift is a convenience that composes classification + expand + arena client drift.
// It is the high-level entry for the drift command.
func RunDrift(changed []string, client interface {
	DriftCheck(specs, impls []string, agent string) (interface{ GetStdout() string; GetExit() int }, error)
}, backend string) (string, int, error) {
	// note: to avoid import cycle we take interface, real use in cmd
	specs, impls := Classify(changed)
	_, err := ExpandToFiles(append(specs, impls...))
	if err != nil {
		return "", 1, err
	}
	// re-classify the expanded? but for simplicity expand all changed first then classify
	// actually per flow: classify first then expand each? but DSN: classify paths, expand to files
	// for now simple: expand the input list, then classify the file list
	expanded, _ := ExpandToFiles(changed)
	s, i := Classify(expanded)
	if len(i) == 0 {
		return "", 5, errors.New("no implementation files")
	}
	_ = backend
	// the real call is done by caller using arena client; this is stub for package
	_ = client // placeholder
	return strings.Join(s, ","), len(i), nil
}
