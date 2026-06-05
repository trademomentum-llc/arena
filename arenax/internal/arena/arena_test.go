package arena

import "testing"

func TestExtractUUID_table(t *testing.T) {
	cases := []struct {
		in  string
		out string
		err bool
	}{
		{`Session created: 123e4567-e89b-12d3-a456-426614174000
Use 'arena run ...'`, "123e4567-e89b-12d3-a456-426614174000", false},
		{"no marker", "", true},
		{"Session created: not-a-uuid", "", true},
	}
	for _, c := range cases {
		u, e := ExtractUUID(c.in)
		if (e != nil) != c.err {
			t.Errorf("in=%q err=%v wantErr=%v", c.in, e, c.err)
		}
		if u != c.out {
			t.Errorf("uuid=%q want %q", u, c.out)
		}
	}
}
