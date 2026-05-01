package main

import "testing"

func TestDefaultChannelFromDir(t *testing.T) {
	tests := []struct {
		name string
		dir  string
		want string
	}{
		{name: "simple basename", dir: "/tmp/rust-rag", want: "rust-rag"},
		{name: "lowercases", dir: "/tmp/ProdOps", want: "prodops"},
		{name: "spaces become dashes", dir: "/tmp/My Project", want: "my-project"},
		{name: "strips unsupported chars", dir: "/tmp/app.v2", want: "app-v2"},
		{name: "empty after cleanup", dir: "/tmp/!!!", want: ""},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := defaultChannelFromDir(tt.dir); got != tt.want {
				t.Fatalf("defaultChannelFromDir(%q) = %q, want %q", tt.dir, got, tt.want)
			}
		})
	}
}
