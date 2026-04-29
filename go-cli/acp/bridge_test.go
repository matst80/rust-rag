package acp

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"net/url"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"
)

func TestToolCallCompletesCurrentStream(t *testing.T) {
	t.Helper()

	var (
		mu      sync.Mutex
		postSeq []string
		patches int
	)

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch {
		case r.Method == http.MethodPost && r.URL.Path == "/api/messages":
			var payload struct {
				Text string `json:"text"`
			}
			if err := json.NewDecoder(r.Body).Decode(&payload); err != nil {
				t.Fatalf("decode post body: %v", err)
			}
			mu.Lock()
			postSeq = append(postSeq, payload.Text)
			id := fmt.Sprintf("msg-%d", len(postSeq))
			mu.Unlock()

			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusCreated)
			_ = json.NewEncoder(w).Encode(apiMessage{
				ID:         id,
				Channel:    "ops",
				Sender:     "bot",
				SenderKind: "agent",
				Text:       payload.Text,
				Kind:       "agent_chunk",
				CreatedAt:  1,
			})
		case r.Method == http.MethodPatch && strings.HasPrefix(r.URL.Path, "/api/messages/"):
			mu.Lock()
			patches++
			mu.Unlock()
			w.WriteHeader(http.StatusOK)
		default:
			t.Fatalf("unexpected request: %s %s", r.Method, r.URL.Path)
		}
	}))
	defer server.Close()

	bridge := NewBridge(BridgeConfig{
		APIURL:  server.URL,
		Channel: "ops",
		BotName: "bot",
	})
	bridge.http = server.Client()

	inst := &AgentInstance{
		name:          "bot",
		bridge:        bridge,
		promptQueue:   make(chan string, 32),
		mentionPrefix: "@bot",
	}
	bridge.instances["bot"] = inst

	inst.dispatch(sessionUpdateMessage(t, "agent_message_chunk", "first"))
	inst.dispatch(sessionUpdateMessage(t, "tool_call", ""))
	inst.dispatch(sessionUpdateMessage(t, "agent_message_chunk", "second"))

	mu.Lock()
	defer mu.Unlock()

	if patches != 0 {
		t.Fatalf("expected tool call to force a new message, got %d patch requests", patches)
	}
	if got, want := len(postSeq), 2; got != want {
		t.Fatalf("expected %d posted messages, got %d", want, got)
	}
	if got, want := postSeq[0], "first"; got != want {
		t.Fatalf("first posted text = %q, want %q", got, want)
	}
	if got, want := postSeq[1], "second"; got != want {
		t.Fatalf("second posted text = %q, want %q", got, want)
	}
}

func TestConsumeMessagesUsesUpdatedAtForCursor(t *testing.T) {
	t.Helper()

	var (
		mu          sync.Mutex
		sinceValues []string
		callCount   int
	)
	base := time.Now().UnixMilli()
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodGet || r.URL.Path != "/api/messages" {
			t.Fatalf("unexpected request: %s %s", r.Method, r.URL.Path)
		}

		query := r.URL.Query()
		mu.Lock()
		sinceValues = append(sinceValues, query.Get("since"))
		callCount++
		currentCall := callCount
		mu.Unlock()

		w.Header().Set("Content-Type", "application/json")
		switch currentCall {
		case 1:
			_ = json.NewEncoder(w).Encode(listMessagesResponse{
				Messages: []apiMessage{{
					ID:         "msg-1",
					Channel:    "ops",
					Sender:     "bot",
					SenderKind: "agent",
					Text:       "updated",
					Kind:       "text",
					CreatedAt:  base + 10,
					UpdatedAt:  base + 20,
				}},
			})
		case 2:
			cancel()
			_ = json.NewEncoder(w).Encode(listMessagesResponse{})
		default:
			t.Fatalf("unexpected extra poll: %d", currentCall)
			_ = json.NewEncoder(w).Encode(listMessagesResponse{})
		}
	}))
	defer server.Close()

	bridge := NewBridge(BridgeConfig{
		APIURL:  server.URL,
		Channel: "ops",
		BotName: "bot",
	})
	bridge.http = server.Client()

	done := make(chan error, 1)
	go func() {
		done <- bridge.consumeMessages(ctx)
	}()

	err := <-done
	if err != context.Canceled {
		t.Fatalf("consumeMessages error = %v, want %v", err, context.Canceled)
	}

	mu.Lock()
	defer mu.Unlock()

	if got, want := len(sinceValues), 2; got != want {
		t.Fatalf("poll count = %d, want %d", got, want)
	}

	firstSince, err := url.QueryUnescape(sinceValues[0])
	if err != nil {
		t.Fatalf("unescape first since: %v", err)
	}
	secondSince, err := url.QueryUnescape(sinceValues[1])
	if err != nil {
		t.Fatalf("unescape second since: %v", err)
	}

	if firstSince == "21" {
		t.Fatalf("first poll since = %q, expected startup cursor not updated cursor", firstSince)
	}
	if got, want := secondSince, fmt.Sprintf("%d", base+20); got != want {
		t.Fatalf("second poll since = %q, want %q", got, want)
	}
}

func TestListMessagesEscapesQueryValues(t *testing.T) {
	t.Helper()

	var gotQuery url.Values
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotQuery = r.URL.Query()
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(listMessagesResponse{})
	}))
	defer server.Close()

	bridge := NewBridge(BridgeConfig{
		APIURL:  server.URL,
		Channel: "ops & support",
		BotName: "bridge bot",
	})
	bridge.http = server.Client()

	if _, err := bridge.listMessages(context.Background(), 123, 25); err != nil {
		t.Fatalf("listMessages error: %v", err)
	}

	if got, want := gotQuery.Get("channel"), "ops & support"; got != want {
		t.Fatalf("channel query = %q, want %q", got, want)
	}
	if got, want := gotQuery.Get("user"), "bridge bot"; got != want {
		t.Fatalf("user query = %q, want %q", got, want)
	}
}

func TestResolveRootChildRejectsEscapes(t *testing.T) {
	t.Helper()

	root := t.TempDir()
	if _, _, err := resolveRootChild(root, "../outside"); err == nil {
		t.Fatalf("expected escape path to be rejected")
	}
	if _, _, err := resolveRootChild(root, filepath.Dir(root)); err == nil {
		t.Fatalf("expected absolute path outside root to be rejected")
	}
}

func TestResolveRootChildAllowsNestedFolder(t *testing.T) {
	t.Helper()

	root := t.TempDir()
	abs, rel, err := resolveRootChild(root, "services/api")
	if err != nil {
		t.Fatalf("resolveRootChild error: %v", err)
	}
	if got, want := rel, filepath.Clean("services/api"); got != want {
		t.Fatalf("rel = %q, want %q", got, want)
	}
	if !strings.HasPrefix(abs, root) {
		t.Fatalf("abs = %q, want path under %q", abs, root)
	}
}

func TestHandleIncomingQueuesPromptsSequentially(t *testing.T) {
	t.Helper()

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	conn := newFakeBridgeConn()
	bridge := NewBridge(BridgeConfig{
		Channel: "ops",
		BotName: "bridge",
	})
	inst := &AgentInstance{
		name:          "bot",
		bridge:        bridge,
		conn:          conn,
		sessionID:     "session-1",
		promptQueue:   make(chan string, 32),
		mentionPrefix: "@bot",
		done:          make(chan struct{}),
	}
	bridge.instances["bot"] = inst

	done := make(chan struct{})
	go func() {
		defer close(done)
		inst.runPromptLoop(ctx)
	}()

	bridge.handleIncoming(ctx, apiMessage{Kind: "text", Text: "@bot first"})
	if got, want := <-conn.started, "first"; got != want {
		t.Fatalf("first started prompt = %q, want %q", got, want)
	}

	bridge.handleIncoming(ctx, apiMessage{Kind: "text", Text: "@bot second"})
	select {
	case got := <-conn.started:
		t.Fatalf("second prompt started before first completed: %q", got)
	case <-time.After(50 * time.Millisecond):
	}

	conn.release <- struct{}{}
	if got, want := <-conn.finished, "first"; got != want {
		t.Fatalf("first finished prompt = %q, want %q", got, want)
	}
	if got, want := <-conn.started, "second"; got != want {
		t.Fatalf("second started prompt = %q, want %q", got, want)
	}

	conn.release <- struct{}{}
	if got, want := <-conn.finished, "second"; got != want {
		t.Fatalf("second finished prompt = %q, want %q", got, want)
	}

	cancel()
	<-done

	conn.mu.Lock()
	defer conn.mu.Unlock()

	if got, want := conn.prompts, []string{"first", "second"}; len(got) != len(want) || got[0] != want[0] || got[1] != want[1] {
		t.Fatalf("prompt order = %v, want %v", got, want)
	}
	if conn.maxActive != 1 {
		t.Fatalf("max concurrent prompts = %d, want 1", conn.maxActive)
	}
}

func TestHandleIncomingRoutesExactMention(t *testing.T) {
	t.Helper()

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	firstConn := newFakeBridgeConn()
	secondConn := newFakeBridgeConn()
	bridge := NewBridge(BridgeConfig{
		Channel: "ops",
		BotName: "bridge",
	})
	first := &AgentInstance{
		name:          "gemini",
		bridge:        bridge,
		conn:          firstConn,
		sessionID:     "session-1",
		promptQueue:   make(chan string, 1),
		mentionPrefix: "@gemini",
		done:          make(chan struct{}),
	}
	second := &AgentInstance{
		name:          "gemini-alpha",
		bridge:        bridge,
		conn:          secondConn,
		sessionID:     "session-2",
		promptQueue:   make(chan string, 1),
		mentionPrefix: "@gemini-alpha",
		done:          make(chan struct{}),
	}
	bridge.instances["gemini"] = first
	bridge.instances["gemini-alpha"] = second

	firstDone := make(chan struct{})
	secondDone := make(chan struct{})
	go func() {
		defer close(firstDone)
		first.runPromptLoop(ctx)
	}()
	go func() {
		defer close(secondDone)
		second.runPromptLoop(ctx)
	}()

	bridge.handleIncoming(ctx, apiMessage{Kind: "text", Text: "@gemini-alpha: hello"})

	select {
	case got := <-firstConn.started:
		t.Fatalf("short mention matched first instance: %q", got)
	case got := <-secondConn.started:
		if got != "hello" {
			t.Fatalf("second started prompt = %q, want %q", got, "hello")
		}
	case <-time.After(200 * time.Millisecond):
		t.Fatalf("no agent received the prompt")
	}

	secondConn.release <- struct{}{}
	cancel()
	<-firstDone
	<-secondDone
}

func TestNextAgentInstanceNamePreservesFirstAndUniquifiesLater(t *testing.T) {
	bridge := NewBridge(BridgeConfig{})
	if got, want := bridge.nextAgentInstanceName("Gemini", true), "gemini"; got != want {
		t.Fatalf("first name = %q, want %q", got, want)
	}
	bridge.instances["gemini"] = &AgentInstance{name: "gemini"}
	got := bridge.nextAgentInstanceName("Gemini", true)
	if got == "gemini" {
		t.Fatalf("expected later instance name to be unique")
	}
	if !strings.HasPrefix(got, "gemini-") {
		t.Fatalf("expected generated name to keep agent prefix, got %q", got)
	}
}

func TestExitCommandRemovesInstanceAndClosesConnection(t *testing.T) {
	t.Helper()

	conn := newFakeBridgeConn()
	bridge := NewBridge(BridgeConfig{
		Channel: "ops",
		BotName: "bridge",
	})
	inst := &AgentInstance{
		name:          "copilot",
		bridge:        bridge,
		conn:          conn,
		sessionID:     "session-1",
		promptQueue:   make(chan string, 1),
		mentionPrefix: "@copilot",
		done:          make(chan struct{}),
	}
	bridge.instances["copilot"] = inst

	inst.handleCommand(context.Background(), "/exit")

	bridge.instanceMu.RLock()
	_, exists := bridge.instances["copilot"]
	bridge.instanceMu.RUnlock()
	if exists {
		t.Fatalf("expected instance to be removed after /exit")
	}
	if !conn.closed {
		t.Fatalf("expected connection to be closed after /exit")
	}
	if err := inst.enqueuePrompt(context.Background(), "hello"); err == nil {
		t.Fatalf("expected closed instance to reject new prompts")
	}
}

func sessionUpdateMessage(t *testing.T, kind, text string) Message {
	t.Helper()

	content, err := json.Marshal(SessionPartData{Type: "text", Text: text})
	if err != nil {
		t.Fatalf("marshal session part: %v", err)
	}
	params, err := json.Marshal(SessionUpdateParams{
		SessionID: "session-1",
		Update: SessionUpdate{
			SessionUpdate: kind,
			Content:       content,
		},
	})
	if err != nil {
		t.Fatalf("marshal session update: %v", err)
	}
	return Message{
		Method: MethodSessionUpdate,
		Params: params,
	}
}

type fakeBridgeConn struct {
	mu        sync.Mutex
	prompts   []string
	active    int
	maxActive int
	started   chan string
	finished  chan string
	release   chan struct{}
	handler   func(Message)
	closed    bool
}

func newFakeBridgeConn() *fakeBridgeConn {
	return &fakeBridgeConn{
		started:  make(chan string, 2),
		finished: make(chan string, 2),
		release:  make(chan struct{}, 2),
	}
}

func (f *fakeBridgeConn) Call(ctx context.Context, method string, params any) (*Message, error) {
	if method != MethodSessionPrompt {
		return &Message{}, nil
	}

	prompt, ok := params.(SessionPromptParams)
	if !ok {
		return nil, fmt.Errorf("unexpected params type %T", params)
	}
	text := prompt.Prompt[0].Text

	f.mu.Lock()
	f.prompts = append(f.prompts, text)
	f.active++
	if f.active > f.maxActive {
		f.maxActive = f.active
	}
	f.mu.Unlock()

	f.started <- text

	select {
	case <-ctx.Done():
		f.mu.Lock()
		f.active--
		f.mu.Unlock()
		return nil, ctx.Err()
	case <-f.release:
	}

	f.mu.Lock()
	f.active--
	f.mu.Unlock()
	f.finished <- text
	return &Message{}, nil
}

func (f *fakeBridgeConn) Notify(context.Context, string, any) error { return nil }

func (f *fakeBridgeConn) Respond(any, any, *RPCError) error { return nil }

func (f *fakeBridgeConn) SetHandler(h func(msg Message)) { f.handler = h }

func (f *fakeBridgeConn) Close() error {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.closed = true
	return nil
}
