package acp

import (
	"fmt"
	"math/rand"
	"strings"
	"time"
)

var adjectives = []string{
	"Quantum", "Spectral", "Emerald", "Obsidian", "Radiant", "Nebula",
	"Vortex", "Crest", "Solar", "Lunar", "Cyber", "Void", "Zenith",
	"Echo", "Prism", "Alpha", "Omega", "Neon", "Cosmic", "Static",
}

var nouns = []string{
	"Phoenix", "Falcon", "Nexus", "Pulse", "Warp", "Spark", "Prism",
	"Ghost", "Titan", "Rider", "Seeker", "Sentry", "Orbit", "Ray",
	"Blade", "Core", "Gate", "Storm", "Flame", "Wave",
}

func generateUniqueName(agentType string) string {
	r := rand.New(rand.NewSource(time.Now().UnixNano()))
	adj := adjectives[r.Intn(len(adjectives))]
	noun := nouns[r.Intn(len(nouns))]
	
	// e.g. gemini-Spectral-Phoenix
	return fmt.Sprintf("%s-%s-%s", strings.ToLower(agentType), strings.ToLower(adj), strings.ToLower(noun))
}
