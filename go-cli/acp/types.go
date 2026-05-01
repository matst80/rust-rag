// Package acp implements a minimal Agent Client Protocol (ACP) stdio client
// adapted from telegram-ai-bot/pkg/acpsvc. Used by the rag CLI to bridge ACP
// agents (gemini, copilot) into the rust-rag messaging system.
package acp

import "encoding/json"

const (
	MethodInitialize               = "initialize"
	MethodInitialized              = "initialized"
	MethodSessionNew               = "session/new"
	MethodSessionPrompt            = "session/prompt"
	MethodSessionCancel            = "session/cancel"
	MethodSessionUpdate            = "session/update"
	MethodSessionRequestPermission = "session/request_permission"

	// Model management extensions
	MethodAgentListModels = "agent/listModels"
	MethodAgentSetModel  = "agent/setModel"
)

type Message struct {
	JSONRPC string          `json:"jsonrpc"`
	Method  string          `json:"method,omitempty"`
	Params  json.RawMessage `json:"params,omitempty"`
	Result  json.RawMessage `json:"result,omitempty"`
	Error   *RPCError       `json:"error,omitempty"`
	ID      any             `json:"id,omitempty"`
}

type RPCError struct {
	Code    int             `json:"code"`
	Message string          `json:"message"`
	Data    json.RawMessage `json:"data,omitempty"`
}

type ClientInfo struct {
	Name    string `json:"name"`
	Version string `json:"version"`
}

type InitializeParams struct {
	ProtocolVersion int        `json:"protocolVersion"`
	ClientInfo      ClientInfo `json:"clientInfo"`
}

type InitializeResult struct {
	ProtocolVersion int             `json:"protocolVersion"`
	AgentInfo       AgentInfo       `json:"agentInfo"`
	AgentCapabilities AgentCapabilities `json:"agentCapabilities,omitempty"`
}

type AgentInfo struct {
	Name    string `json:"name"`
	Title   string `json:"title"`
	Version string `json:"version"`
}

type AgentCapabilities struct {
	Models bool `json:"models,omitempty"`
}

type AgentListModelsResult struct {
	Models []AgentModel `json:"models"`
}

type AgentModel struct {
	ID          string `json:"id"`
	Name        string `json:"name"`
	Description string `json:"description,omitempty"`
}

type AgentSetModelParams struct {
	SessionID string `json:"sessionId,omitempty"`
	ModelID   string `json:"modelId"`
}

type SessionNewParams struct {
	Title      string                `json:"title,omitempty"`
	CWD        string                `json:"cwd"`
	MCPServers []MCPServerDefinition `json:"mcpServers"`
}

type MCPServerDefinition struct {
	Name string `json:"name"`
	Type string `json:"type"`
	URL  string `json:"url,omitempty"`
}

type SessionNewResult struct {
	SessionID string `json:"sessionId"`
}

type SessionPartData struct {
	Type string `json:"type"`
	Text string `json:"text,omitempty"`
}

type SessionPromptParams struct {
	SessionID string            `json:"sessionId"`
	Prompt    []SessionPartData `json:"prompt"`
}

type SessionUpdateParams struct {
	SessionID string        `json:"sessionId"`
	Update    SessionUpdate `json:"update"`
}

type SessionUpdate struct {
	SessionUpdate string          `json:"sessionUpdate"`
	Content       json.RawMessage `json:"content,omitempty"`
	ToolCallID    string          `json:"toolCallId,omitempty"`
	Title         string          `json:"title,omitempty"`
	Status        string          `json:"status,omitempty"`
	Kind          string          `json:"kind,omitempty"`
}

// GetText extracts plain text from a SessionUpdate's Content payload, which
// may be a single SessionPartData or an array of such parts (sometimes wrapped).
func (u SessionUpdate) GetText() string {
	if len(u.Content) == 0 {
		return ""
	}
	var part SessionPartData
	if err := json.Unmarshal(u.Content, &part); err == nil && part.Text != "" {
		return part.Text
	}
	var arr []json.RawMessage
	if err := json.Unmarshal(u.Content, &arr); err == nil {
		var combined string
		for _, item := range arr {
			var p SessionPartData
			if err := json.Unmarshal(item, &p); err == nil && p.Text != "" {
				combined += p.Text
				continue
			}
			var nested struct {
				Content SessionPartData `json:"content"`
			}
			if err := json.Unmarshal(item, &nested); err == nil && nested.Content.Text != "" {
				combined += nested.Content.Text
			}
		}
		return combined
	}
	return ""
}

type PermissionRequestParams struct {
	SessionID string             `json:"sessionId"`
	ToolCall  *ToolCallInfo      `json:"toolCall,omitempty"`
	Options   []PermissionOption `json:"options"`
}

type ToolCallInfo struct {
	ToolCallID string          `json:"toolCallId"`
	Title      string          `json:"title"`
	Kind       string          `json:"kind"`
	RawInput   json.RawMessage `json:"rawInput,omitempty"`
	Content    json.RawMessage `json:"content,omitempty"`
}

type PermissionOption struct {
	OptionID string `json:"optionId"`
	Kind     string `json:"kind"`
	Name     string `json:"name"`
}

type PermissionResponse struct {
	Outcome PermissionOutcome `json:"outcome"`
}

type PermissionOutcome struct {
	Outcome  string `json:"outcome"` // "selected" | "cancelled"
	OptionID string `json:"optionId,omitempty"`
}
