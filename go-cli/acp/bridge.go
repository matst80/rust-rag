package acp

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"
)

// BridgeConfig parameters for the messaging<->ACP bridge.
type BridgeConfig struct {
	APIURL      string
	AccessToken string
	Channel     string
	BotName     string // sender label for bridge-posted messages
	Root        string // if set, allows scanning and spawning in subfolders
	Debug       bool

	// Static agent (legacy/simple mode)
	AgentName string
	Command   string
	Args      []string
	CWD       string

	// Managed agents
	Agents map[string]AgentSpec
}

type AgentSpec struct {
	Command string
	Args    []string
}

// listMessagesResponse mirrors the rust-rag MessagesResponse shape; we keep it
// inline to avoid importing the frontend types.
type listMessagesResponse struct {
	Messages    []apiMessage `json:"messages"`
	TotalCount  int64        `json:"total_count"`
	ActiveUsers []any        `json:"active_users"`
	DeletedIDs  []string     `json:"deleted_ids"`
}

type apiMessage struct {
	ID         string         `json:"id"`
	Channel    string         `json:"channel"`
	Sender     string         `json:"sender"`
	SenderKind string         `json:"sender_kind"`
	Text       string         `json:"text"`
	Kind       string         `json:"kind"`
	Metadata   map[string]any `json:"metadata"`
	CreatedAt  int64          `json:"created_at"`
	UpdatedAt  int64          `json:"updated_at"`
}

func (m apiMessage) cursorTimestamp() int64 {
	if m.UpdatedAt > m.CreatedAt {
		return m.UpdatedAt
	}
	return m.CreatedAt
}

type rpcConn interface {
	Call(ctx context.Context, method string, params any) (*Message, error)
	Notify(ctx context.Context, method string, params any) error
	Respond(id any, result any, rpcErr *RPCError) error
	SetHandler(func(msg Message))
	Close() error
}

// Bridge ties the rust-rag messaging API to a single ACP agent process.
type Bridge struct {
	cfg       BridgeConfig
	http      *http.Client
	permMu    sync.Mutex
	permWaits map[string]chan string

	instanceMu sync.RWMutex
	instances  map[string]*AgentInstance // name (lowercase) -> instance

	mentionPrefix string // manager's mention prefix
}

type AgentInstance struct {
	name          string
	agentType     string
	conn          rpcConn
	promptQueue   chan string
	mentionPrefix string
	sessionMu     sync.RWMutex
	sessionID     string
	sessions      map[string]string // folder -> sessionID
	closeOnce     sync.Once
	done          chan struct{}

	// streaming agent reply
	streamMu      sync.Mutex
	streamID      string
	streamHasText bool

	bridge *Bridge
}

func NewBridge(cfg BridgeConfig) *Bridge {
	return &Bridge{
		cfg:           cfg,
		http:          &http.Client{Timeout: 60 * time.Second},
		permWaits:     make(map[string]chan string),
		instances:     make(map[string]*AgentInstance),
		mentionPrefix: "@" + strings.ToLower(cfg.BotName),
	}
}

func (b *Bridge) Run(ctx context.Context) error {
	_ = b.postSystem("bridge online — ready for commands")

	if b.cfg.Command != "" {
		if _, err := b.startAgent(ctx, b.cfg.AgentName, b.cfg.Command, b.cfg.Args, b.cfg.CWD); err != nil {
			log.Printf("[bridge] failed to start initial agent %s: %v", b.cfg.AgentName, err)
		}
	} else if len(b.cfg.Agents) > 0 {
		var names []string
		for k := range b.cfg.Agents {
			names = append(names, k)
		}
		_ = b.postSystem("bridge ready. available agents: " + strings.Join(names, ", ") + ". use `@bot use <agent>` to start.")
	}

	if b.cfg.Root != "" {
		go b.announceRoot(ctx)
	}

	return b.consumeMessages(ctx)
}

func (b *Bridge) startAgent(ctx context.Context, name, command string, args []string, cwd string) (*AgentInstance, error) {
	conn, err := NewStdioConnection(ctx, name, command, args, b.cfg.Debug)
	if err != nil {
		return nil, fmt.Errorf("spawn %s: %w", name, err)
	}
	started := false
	defer func() {
		if !started {
			_ = conn.Close()
		}
	}()

	inst := &AgentInstance{
		name:          name,
		agentType:     name,
		conn:          conn,
		promptQueue:   make(chan string, 32),
		mentionPrefix: "@" + strings.ToLower(name),
		sessions:      make(map[string]string),
		done:          make(chan struct{}),
		bridge:        b,
	}

	conn.SetHandler(func(msg Message) {
		inst.dispatch(msg)
	})

	// Initialize.
	if _, err := conn.Call(ctx, MethodInitialize, InitializeParams{
		ProtocolVersion: 1,
		ClientInfo:      ClientInfo{Name: "rag-cli", Version: "0.1.0"},
	}); err != nil {
		return nil, fmt.Errorf("initialize: %w", err)
	}
	if err := conn.Notify(ctx, MethodInitialized, nil); err != nil {
		return nil, fmt.Errorf("initialized notify: %w", err)
	}

	// New session.
	if cwd == "" {
		cwd, _ = os.Getwd()
	}
	res, err := conn.Call(ctx, MethodSessionNew, SessionNewParams{
		Title:      fmt.Sprintf("rag bridge: %s", name),
		CWD:        cwd,
		MCPServers: []MCPServerDefinition{},
	})
	if err != nil {
		return nil, fmt.Errorf("session/new: %w", err)
	}
	var sn SessionNewResult
	if err := json.Unmarshal(res.Result, &sn); err != nil {
		return nil, fmt.Errorf("session/new parse: %w", err)
	}

	if b.cfg.Root != "" {
		inst.setSession("(root)", sn.SessionID)
	} else {
		inst.setSession(filepath.Base(cwd), sn.SessionID)
	}

	b.instanceMu.Lock()
	b.instances[strings.ToLower(name)] = inst
	b.instanceMu.Unlock()

	go inst.runPromptLoop(ctx)
	started = true

	_ = b.postSystem(fmt.Sprintf("agent `%s` online — mention me as `%s`", name, inst.mentionPrefix))
	return inst, nil
}

func (b *Bridge) handleUseAgent(ctx context.Context, name string) {
	spec, ok := b.cfg.Agents[name]
	if !ok {
		_ = b.postSystem(fmt.Sprintf("unknown agent: %s. available: %v", name, b.getAgentNames()))
		return
	}
	instanceName := b.nextAgentInstanceName(name, true)
	if _, err := b.startAgent(ctx, instanceName, spec.Command, spec.Args, ""); err != nil {
		_ = b.postSystem(fmt.Sprintf("failed to start agent %s: %v", instanceName, err))
	}
}

func (b *Bridge) handleSpawnWithAgent(ctx context.Context, agentType, folder string) {
	spec, ok := b.cfg.Agents[agentType]
	if !ok {
		_ = b.postSystem(fmt.Sprintf("unknown agent: %s", agentType))
		return
	}
	abs, _, err := resolveRootChild(b.cfg.Root, folder)
	if err != nil {
		_ = b.postSystem("invalid folder: " + folder)
		return
	}
	name := b.nextAgentInstanceName(agentType, false)
	if _, err := b.startAgent(ctx, name, spec.Command, spec.Args, abs); err != nil {
		_ = b.postSystem(fmt.Sprintf("failed to spawn %s in %s: %v", agentType, folder, err))
	}
}

func (b *Bridge) handleStatus(ctx context.Context) {
	b.instanceMu.RLock()
	defer b.instanceMu.RUnlock()

	if len(b.instances) == 0 {
		_ = b.postSystem("status: no agents running. available: " + strings.Join(b.getAgentNames(), ", "))
		return
	}

	var sb strings.Builder
	sb.WriteString("status: active agents:\n")
	for _, inst := range b.instances {
		sb.WriteString(fmt.Sprintf("- `%s` (%s): session=%s\n", inst.name, inst.agentType, inst.currentSessionID()))
	}
	_ = b.postSystem(sb.String())
}

func (b *Bridge) getAgentNames() []string {
	var names []string
	for k := range b.cfg.Agents {
		names = append(names, k)
	}
	return names
}

func (b *Bridge) announceRoot(ctx context.Context) {
	dirs, err := b.scanRoot()
	if err != nil {
		log.Printf("[bridge] scan root err: %v", err)
		return
	}
	if len(dirs) == 0 {
		return
	}

	var sb strings.Builder
	sb.WriteString("found project directories in `" + b.cfg.Root + "`:\n")
	for _, d := range dirs {
		sb.WriteString(fmt.Sprintf("- `%s`\n", d))
	}
	sb.WriteString("\nuse `" + b.mentionPrefix + " spawn <folder>` to start a session in a subdirectory.")

	meta := map[string]any{
		"root":    b.cfg.Root,
		"folders": dirs,
		"agents":  b.getAgentNames(),
	}
	_ = b.postKind("agent_root_discovery", sb.String(), meta)
}

func (b *Bridge) scanRoot() ([]string, error) {
	entries, err := os.ReadDir(b.cfg.Root)
	if err != nil {
		return nil, err
	}
	var dirs []string
	for _, entry := range entries {
		if entry.IsDir() && !strings.HasPrefix(entry.Name(), ".") {
			dirs = append(dirs, entry.Name())
		}
	}
	return dirs, nil
}

// consumeMessages runs the long-poll loop. It calls /api/messages with wait=25
// and `since` set to the last seen message activity timestamp so updated rows
// do not replay forever. Messages from this bridge's own bot user are skipped
// to avoid loops.
func (b *Bridge) consumeMessages(ctx context.Context) error {
	var lastSeen int64 = time.Now().UnixMilli()
	seenAtCursor := make(map[string]struct{})
	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		default:
		}

		resp, err := b.listMessages(ctx, lastSeen, 25)
		if err != nil {
			log.Printf("[bridge] poll err: %v", err)
			time.Sleep(2 * time.Second)
			continue
		}

		for _, m := range resp.Messages {
			ts := m.cursorTimestamp()
			switch {
			case ts < lastSeen:
				continue
			case ts > lastSeen:
				lastSeen = ts
				seenAtCursor = make(map[string]struct{})
			}
			if _, ok := seenAtCursor[m.ID]; ok {
				continue
			}
			seenAtCursor[m.ID] = struct{}{}
			if m.Sender == b.cfg.BotName {
				continue
			}
			b.handleIncoming(ctx, m)
		}
	}
}

func (b *Bridge) handleIncoming(ctx context.Context, m apiMessage) {
	body := strings.TrimSpace(m.Text)
	lower := strings.ToLower(body)
	mention, stripped, ok := splitLeadingMention(lower, body)

	switch m.Kind {
	case "permission_response":
		reqID, _ := m.Metadata["request_id"].(string)
		optID, _ := m.Metadata["option_id"].(string)
		if reqID == "" || optID == "" {
			return
		}
		b.permMu.Lock()
		ch, ok := b.permWaits[reqID]
		if ok {
			delete(b.permWaits, reqID)
		}
		b.permMu.Unlock()
		if ok {
			select {
			case ch <- optID:
			default:
			}
		}
	case "text", "":
		if body == "" {
			return
		}

		// 1. Check for manager mentions (@bridge)
		if ok && mention == b.mentionPrefix {
			if stripped == "" {
				return
			}

			// Handle manager commands
			if strings.HasPrefix(stripped, "/model ") {
				_ = b.postSystem("manager does not support /model. mention a specific agent.")
				return
			}
			if strings.HasPrefix(stripped, "use ") {
				agentName := strings.TrimSpace(strings.TrimPrefix(stripped, "use "))
				go b.handleUseAgent(ctx, agentName)
				return
			}
			if strings.HasPrefix(stripped, "spawn ") {
				args := strings.Fields(strings.TrimPrefix(stripped, "spawn "))
				if len(args) == 0 {
					return
				}
				if len(args) >= 2 {
					if _, ok := b.cfg.Agents[args[0]]; ok {
						go b.handleSpawnWithAgent(ctx, args[0], args[1])
						return
					}
				}
				agentType := ""
				for k := range b.cfg.Agents {
					agentType = k
					break
				}
				if agentType == "" {
					_ = b.postSystem("no agents configured")
					return
				}
				go b.handleSpawnWithAgent(ctx, agentType, args[0])
				return
			}
			if stripped == "sessions" {
				_ = b.postSystem("manager does not support sessions. mention a specific agent.")
				return
			}
			if stripped == "status" {
				go b.handleStatus(ctx)
				return
			}
			_ = b.postSystem("unknown command. available: use, spawn, status")
			return
		}

		// 2. Check for agent mentions (@agent-adj-noun)
		if !ok {
			return
		}
		b.instanceMu.RLock()
		target := b.instances[strings.TrimPrefix(mention, "@")]
		b.instanceMu.RUnlock()

		if target != nil {
			if stripped == "" {
				return
			}
			target.handleCommand(ctx, stripped)
		}
	}
}

func (inst *AgentInstance) handleCommand(ctx context.Context, stripped string) {
	// Handle /model and /skill commands
	if strings.HasPrefix(stripped, "/") {
		parts := strings.Fields(stripped)
		cmd := parts[0]
		args := parts[1:]

		if cmd == "/exit" {
			inst.handleExit()
			return
		}

		if cmd == "/model" {
			if len(args) == 0 || args[0] == "list" {
				go inst.handleListModels(ctx)
				return
			}
			if args[0] == "set" && len(args) >= 2 {
				go inst.handleSetModel(ctx, args[1])
				return
			}
		}

		if cmd == "/skill" {
			if len(args) == 0 || args[0] == "list" {
				go inst.handleListSkills(ctx)
				return
			}
			go inst.handleLoadSkill(ctx, args[0])
			return
		}
	}

	if strings.HasPrefix(stripped, "spawn ") {
		folder := strings.TrimSpace(strings.TrimPrefix(stripped, "spawn "))
		go inst.handleSpawn(ctx, folder)
		return
	}

	if stripped == "sessions" {
		go inst.handleListSessions(ctx)
		return
	}

	if err := inst.enqueuePrompt(ctx, stripped); err != nil && ctx.Err() == nil {
		log.Printf("[bridge][%s] enqueue prompt err: %v", inst.name, err)
	}
}

func (inst *AgentInstance) handleExit() {
	inst.closeOnce.Do(func() {
		close(inst.done)
		inst.bridge.instanceMu.Lock()
		delete(inst.bridge.instances, strings.ToLower(inst.name))
		inst.bridge.instanceMu.Unlock()
		_ = inst.conn.Close()
		_ = inst.bridge.postSystem(fmt.Sprintf("agent `%s` exited", inst.name))
	})
}

func (inst *AgentInstance) handleSpawn(ctx context.Context, folder string) {
	if inst.bridge.cfg.Root == "" {
		_ = inst.postSystem("no root folder configured")
		return
	}

	path, rel, err := resolveRootChild(inst.bridge.cfg.Root, folder)
	if err != nil {
		_ = inst.postSystem(fmt.Sprintf("folder `%s` not found in root", folder))
		return
	}
	info, err := os.Stat(path)
	if err != nil || !info.IsDir() {
		_ = inst.postSystem(fmt.Sprintf("folder `%s` not found in root", folder))
		return
	}

	_ = inst.postSystem(fmt.Sprintf("spawning new session in `%s`...", rel))

	res, err := inst.conn.Call(ctx, MethodSessionNew, SessionNewParams{
		Title:      fmt.Sprintf("rag bridge: %s", folder),
		CWD:        path,
		MCPServers: []MCPServerDefinition{},
	})
	if err != nil {
		_ = inst.postSystem("failed to spawn session: " + err.Error())
		return
	}

	var sn SessionNewResult
	if err := json.Unmarshal(res.Result, &sn); err != nil {
		_ = inst.postSystem("failed to parse session result")
		return
	}

	inst.setSession(rel, sn.SessionID)

	_ = inst.postSystem(fmt.Sprintf("switched to session `%s` in `%s`", sn.SessionID, rel))
}

func (inst *AgentInstance) handleListSessions(ctx context.Context) {
	currentID, sessions := inst.snapshotSessions()
	var sb strings.Builder
	sb.WriteString(fmt.Sprintf("active sessions for `%s`:\n", inst.name))
	for folder, id := range sessions {
		active := ""
		if id == currentID {
			active = " (active)"
		}
		sb.WriteString(fmt.Sprintf("- `%s`: `%s`%s\n", folder, id, active))
	}
	_ = inst.postSystem(sb.String())
}

func (inst *AgentInstance) handleListModels(ctx context.Context) {
	res, err := inst.conn.Call(ctx, MethodAgentListModels, nil)
	if err != nil {
		_ = inst.postSystem("failed to list models: " + err.Error())
		return
	}
	var lr AgentListModelsResult
	if err := json.Unmarshal(res.Result, &lr); err != nil {
		_ = inst.postSystem("failed to parse models result")
		return
	}
	var sb strings.Builder
	sb.WriteString("available models:\n")
	for _, m := range lr.Models {
		sb.WriteString(fmt.Sprintf("- `%s`: %s\n", m.ID, m.Name))
	}
	_ = inst.postSystem(sb.String())
}

func (inst *AgentInstance) handleSetModel(ctx context.Context, modelID string) {
	_, err := inst.conn.Call(ctx, MethodAgentSetModel, AgentSetModelParams{
		SessionID: inst.currentSessionID(),
		ModelID:   modelID,
	})
	if err != nil {
		_ = inst.postSystem(fmt.Sprintf("failed to set model to `%s`: %v", modelID, err))
		return
	}
	_ = inst.postSystem(fmt.Sprintf("model changed to `%s`", modelID))
}

func (inst *AgentInstance) handleListSkills(ctx context.Context) {
	root := inst.bridge.cfg.Root
	if root == "" {
		root = "."
	}
	skillsDir := filepath.Join(root, "skills")
	entries, err := os.ReadDir(skillsDir)
	if err != nil {
		_ = inst.postSystem("no skills directory found in " + root)
		return
	}

	var skills []string
	for _, entry := range entries {
		if !entry.IsDir() && strings.HasSuffix(entry.Name(), ".md") {
			skills = append(skills, strings.TrimSuffix(entry.Name(), ".md"))
		}
	}

	if len(skills) == 0 {
		_ = inst.postSystem("no skills found in " + skillsDir)
		return
	}

	_ = inst.postSystem("available skills:\n- " + strings.Join(skills, "\n- "))
}

func (inst *AgentInstance) handleLoadSkill(ctx context.Context, skillName string) {
	root := inst.bridge.cfg.Root
	if root == "" {
		root = "."
	}
	path := filepath.Join(root, "skills", skillName+".md")
	content, err := os.ReadFile(path)
	if err != nil {
		_ = inst.postSystem(fmt.Sprintf("skill `%s` not found in `%s/skills/`", skillName, root))
		return
	}

	_ = inst.postSystem(fmt.Sprintf("loading skill `%s`...", skillName))

	prompt := fmt.Sprintf("### LOADED SKILL: %s\n\nPlease adopt the following instructions/capabilities:\n\n%s", skillName, string(content))
	if err := inst.enqueuePrompt(ctx, prompt); err != nil {
		_ = inst.postSystem("failed to enqueue skill prompt")
	}
}

func (inst *AgentInstance) sendPrompt(ctx context.Context, text string) {
	sessionID := inst.currentSessionID()
	if sessionID == "" {
		return
	}
	// Reset the streaming buffer for this turn — any prior chunks are flushed.
	inst.resetStream()

	_, err := inst.conn.Call(ctx, MethodSessionPrompt, SessionPromptParams{
		SessionID: sessionID,
		Prompt:    []SessionPartData{{Type: "text", Text: text}},
	})
	if err != nil {
		log.Printf("[bridge][%s] prompt err: %v", inst.name, err)
		_ = inst.postSystem("prompt error: " + err.Error())
	}

	// Prompt returned: close the current streaming message so the next turn
	// starts a fresh row.
	inst.resetStream()
}

func (inst *AgentInstance) enqueuePrompt(ctx context.Context, text string) error {
	select {
	case <-inst.done:
		return fmt.Errorf("agent %s is closed", inst.name)
	default:
	}
	select {
	case <-inst.done:
		return fmt.Errorf("agent %s is closed", inst.name)
	case inst.promptQueue <- text:
		return nil
	case <-ctx.Done():
		return ctx.Err()
	}
}

func (inst *AgentInstance) runPromptLoop(ctx context.Context) {
	for {
		select {
		case <-inst.done:
			return
		case <-ctx.Done():
			return
		case text := <-inst.promptQueue:
			inst.sendPrompt(ctx, text)
		}
	}
}

func (inst *AgentInstance) appendStreamChunk(text string) error {
	inst.streamMu.Lock()
	id := inst.streamID
	inst.streamMu.Unlock()

	if id == "" {
		created, err := inst.postKindReturning("agent_chunk", text, nil)
		if err != nil {
			return err
		}
		inst.streamMu.Lock()
		inst.streamID = created.ID
		inst.streamHasText = text != ""
		inst.streamMu.Unlock()
		return nil
	}
	// Patch with append=true so the server appends server-side and bumps updated_at.
	return inst.bridge.patchMessage(id, &text, nil, true)
}

func (inst *AgentInstance) resetStream() {
	inst.streamMu.Lock()
	inst.streamID = ""
	inst.streamHasText = false
	inst.streamMu.Unlock()
}

func (inst *AgentInstance) dispatch(msg Message) {
	ctx := context.Background()
	if msg.Method == "" {
		return
	}
	switch msg.Method {
	case MethodSessionUpdate:
		var p SessionUpdateParams
		if err := json.Unmarshal(msg.Params, &p); err != nil {
			return
		}
		switch p.Update.SessionUpdate {
		case "agent_message_chunk":
			text := p.Update.GetText()
			if text != "" {
				if err := inst.appendStreamChunk(text); err != nil {
					log.Printf("[bridge][%s] stream append err: %v", inst.name, err)
				}
			}
		case "agent_thought_chunk":
		case "tool_call", "tool_call_update":
			inst.resetStream()
		}
	case MethodSessionRequestPermission:
		var p PermissionRequestParams
		if err := json.Unmarshal(msg.Params, &p); err != nil {
			inst.conn.Respond(msg.ID, nil, &RPCError{Code: -32602, Message: "bad params"})
			return
		}
		go inst.handlePermissionRequest(ctx, msg.ID, p)
	}
}

func (inst *AgentInstance) handlePermissionRequest(ctx context.Context, rpcID any, p PermissionRequestParams) {
	requestID := fmt.Sprintf("perm-%s-%d", inst.name, time.Now().UnixNano())
	wait := make(chan string, 1)
	inst.bridge.permMu.Lock()
	inst.bridge.permWaits[requestID] = wait
	inst.bridge.permMu.Unlock()

	options := make([]map[string]any, 0, len(p.Options))
	for _, opt := range p.Options {
		options = append(options, map[string]any{
			"option_id": opt.OptionID,
			"name":      opt.Name,
			"kind":      opt.Kind,
		})
	}
	body := "permission required"
	if p.ToolCall != nil {
		body = fmt.Sprintf("[%s] permission required for: %s", inst.name, p.ToolCall.Title)
	}
	meta := map[string]any{
		"request_id": requestID,
		"options":    options,
	}
	if p.ToolCall != nil {
		meta["tool_call"] = map[string]any{
			"title": p.ToolCall.Title,
			"kind":  p.ToolCall.Kind,
		}
	}

	if err := inst.postKind("permission_request", body, meta); err != nil {
		log.Printf("[bridge][%s] post permission_request: %v", inst.name, err)
		inst.conn.Respond(rpcID, nil, &RPCError{Code: -32000, Message: err.Error()})
		return
	}

	timeout := 5 * time.Minute
	select {
	case <-ctx.Done():
		inst.bridge.permMu.Lock()
		delete(inst.bridge.permWaits, requestID)
		inst.bridge.permMu.Unlock()
		inst.conn.Respond(rpcID, PermissionResponse{Outcome: PermissionOutcome{Outcome: "cancelled"}}, nil)
	case <-time.After(timeout):
		inst.bridge.permMu.Lock()
		delete(inst.bridge.permWaits, requestID)
		inst.bridge.permMu.Unlock()
		_ = inst.postSystem("permission request timed out")
		inst.conn.Respond(rpcID, PermissionResponse{Outcome: PermissionOutcome{Outcome: "cancelled"}}, nil)
	case optID := <-wait:
		inst.conn.Respond(rpcID, PermissionResponse{Outcome: PermissionOutcome{Outcome: "selected", OptionID: optID}}, nil)
	}
}

func (inst *AgentInstance) postSystem(text string) error {
	return inst.postKind("text", text, nil)
}

func (inst *AgentInstance) postKind(kind, text string, metadata map[string]any) error {
	_, err := inst.postKindReturning(kind, text, metadata)
	return err
}

func (inst *AgentInstance) postKindReturning(kind, text string, metadata map[string]any) (*apiMessage, error) {
	return inst.bridge.postKindReturningAs(kind, text, metadata, inst.name)
}

func (b *Bridge) listMessages(ctx context.Context, since int64, waitSecs int) (*listMessagesResponse, error) {
	query := url.Values{}
	query.Set("channel", b.cfg.Channel)
	query.Set("since", fmt.Sprintf("%d", since))
	query.Set("limit", "200")
	query.Set("sort_order", "asc")
	query.Set("wait", fmt.Sprintf("%d", waitSecs))
	query.Set("user", b.cfg.BotName)
	query.Set("user_kind", "agent")
	endpoint := strings.TrimRight(b.cfg.APIURL, "/") + "/api/messages?" + query.Encode()
	req, _ := http.NewRequestWithContext(ctx, http.MethodGet, endpoint, nil)
	if b.cfg.AccessToken != "" {
		req.Header.Set("Authorization", "Bearer "+b.cfg.AccessToken)
	}
	resp, err := b.http.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("list messages: %s: %s", resp.Status, string(body))
	}
	var out listMessagesResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return nil, err
	}
	return &out, nil
}

func (b *Bridge) postKind(kind, text string, metadata map[string]any) error {
	_, err := b.postKindReturningAs(kind, text, metadata, b.cfg.BotName)
	return err
}

func (b *Bridge) postSystem(text string) error {
	return b.postKind("text", text, nil)
}

func (b *Bridge) postKindReturningAs(kind, text string, metadata map[string]any, sender string) (*apiMessage, error) {
	payload := map[string]any{
		"channel":     b.cfg.Channel,
		"text":        text,
		"sender":      sender,
		"sender_kind": "agent",
		"kind":        kind,
	}
	if metadata != nil {
		payload["metadata"] = metadata
	}
	body, _ := json.Marshal(payload)
	req, _ := http.NewRequest(http.MethodPost, b.cfg.APIURL+"/api/messages", bytes.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	if b.cfg.AccessToken != "" {
		req.Header.Set("Authorization", "Bearer "+b.cfg.AccessToken)
	}
	resp, err := b.http.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusCreated && resp.StatusCode != http.StatusOK {
		buf, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("post: %s: %s", resp.Status, string(buf))
	}
	var out apiMessage
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return nil, err
	}
	return &out, nil
}

func (b *Bridge) patchMessage(id string, text *string, metadata map[string]any, append bool) error {
	payload := map[string]any{}
	if text != nil {
		payload["text"] = *text
	}
	if metadata != nil {
		payload["metadata"] = metadata
	}
	if append {
		payload["append"] = true
	}
	body, _ := json.Marshal(payload)
	req, _ := http.NewRequest(http.MethodPatch, fmt.Sprintf("%s/api/messages/%s", b.cfg.APIURL, id), bytes.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	if b.cfg.AccessToken != "" {
		req.Header.Set("Authorization", "Bearer "+b.cfg.AccessToken)
	}
	resp, err := b.http.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		buf, _ := io.ReadAll(resp.Body)
		return fmt.Errorf("patch: %s: %s", resp.Status, string(buf))
	}
	return nil
}

func (inst *AgentInstance) currentSessionID() string {
	inst.sessionMu.RLock()
	defer inst.sessionMu.RUnlock()
	return inst.sessionID
}

func (inst *AgentInstance) setSession(folder, sessionID string) {
	inst.sessionMu.Lock()
	defer inst.sessionMu.Unlock()
	inst.sessionID = sessionID
	inst.sessions[folder] = sessionID
}

func (inst *AgentInstance) snapshotSessions() (string, map[string]string) {
	inst.sessionMu.RLock()
	defer inst.sessionMu.RUnlock()
	out := make(map[string]string, len(inst.sessions))
	for folder, id := range inst.sessions {
		out[folder] = id
	}
	return inst.sessionID, out
}

func resolveRootChild(root, child string) (abs string, rel string, err error) {
	if strings.TrimSpace(root) == "" {
		return "", "", fmt.Errorf("root is not configured")
	}
	if strings.TrimSpace(child) == "" {
		return "", "", fmt.Errorf("child path is empty")
	}
	if filepath.IsAbs(child) {
		return "", "", fmt.Errorf("absolute paths are not allowed")
	}
	rootAbs, err := filepath.Abs(root)
	if err != nil {
		return "", "", err
	}
	targetAbs, err := filepath.Abs(filepath.Join(rootAbs, child))
	if err != nil {
		return "", "", err
	}
	rel, err = filepath.Rel(rootAbs, targetAbs)
	if err != nil {
		return "", "", err
	}
	if rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
		return "", "", fmt.Errorf("path escapes root")
	}
	return targetAbs, filepath.Clean(rel), nil
}

func splitLeadingMention(lower, original string) (mention string, remainder string, ok bool) {
	if !strings.HasPrefix(lower, "@") {
		return "", "", false
	}
	end := 1
	for end < len(lower) {
		switch lower[end] {
		case ' ', '\t', '\n', '\r', ':', ',':
			goto done
		default:
			end++
		}
	}
done:
	mention = lower[:end]
	remainder = strings.TrimSpace(original[end:])
	remainder = strings.TrimLeft(remainder, ":, \t")
	return mention, remainder, mention != "@"
}

func (b *Bridge) nextAgentInstanceName(agentType string, preferBase bool) string {
	base := strings.ToLower(strings.TrimSpace(agentType))
	if base == "" {
		base = "agent"
	}
	b.instanceMu.RLock()
	_, exists := b.instances[base]
	b.instanceMu.RUnlock()
	if preferBase && !exists {
		return base
	}
	for {
		name := generateUniqueName(base)
		b.instanceMu.RLock()
		_, exists := b.instances[strings.ToLower(name)]
		b.instanceMu.RUnlock()
		if !exists {
			return name
		}
	}
}
