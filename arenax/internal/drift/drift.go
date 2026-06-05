// internal/drift/drift.go - classification and drift per spec DSN 4.2 / TEC 5.2
package drift

import (
	"os"
	"path/filepath"
	"strings"
)

type Ruleset struct{} // for future

func Classify(paths []string) (specs, impls []string) {
	for _, p := range paths {
		if isSpec(p) {
			specs = append(specs, p)
		} else if isImpl(p) {
			impls = append(impls, p)
		} else {
			// ignored (logged in real)
		}
	}
	return
}

func isSpec(p string) bool {
	lower := strings.ToLower(p)
	if strings.HasPrefix(lower, "docs/") || strings.HasPrefix(lower, "spec/") || strings.HasPrefix(lower, "specs/") {
		return true
	}
	ext := filepath.Ext(lower)
	if (ext == ".md" || ext == ".yaml" || ext == ".yml" || ext == ".json") && strings.Contains(lower, "spec") {
		return true
	}
	return false
}

func isImpl(p string) bool {
	lower := strings.ToLower(p)
	return strings.HasPrefix(lower, "src/") || strings.HasPrefix(lower, "lib/") || strings.HasPrefix(lower, "internal/")
}

func ExpandToFiles(paths []string) []string {
	var out []string
	for _, p := range paths {
		info, err := os.Stat(p)
		if err != nil {
			continue
		}
		if info.IsDir() {
			filepath.Walk(p, func(path string, info os.FileInfo, err error) error {
				if err == nil && !info.IsDir() {
					out = append(out, path)
				}
				return nil
			})
		} else {
			out = append(out, p)
		}
	}
	return out
}

func RunDrift(specs, impls []string, agent string) string {
	// placeholder; real delegates to arena client
	return "Drift run for " + strings.Join(impls, ",") + " vs specs"
}
