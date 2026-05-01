package main

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"regexp"
	"strings"
	"syscall"
	"time"

	"github.com/matst80/rust-rag/go-cli/acp"
	"github.com/spf13/cobra"
	"github.com/spf13/viper"
)

type Config struct {
	APIURL      string `json:"api_url"`
	AccessToken string `json:"access_token"`
	Channel     string `json:"channel"`
}

var cfgFile string

func main() {
	Execute()
}

func Execute() {
	var rootCmd = &cobra.Command{
		Use:   "rag",
		Short: "RAG CLI tool to interact with rust-rag API",
	}

	rootCmd.PersistentFlags().StringVar(&cfgFile, "config", "", "config file (default is $HOME/.config/rust-rag/config.json)")
	rootCmd.PersistentFlags().String("api-url", "http://localhost:3000", "Base URL of the RAG API")
	rootCmd.PersistentFlags().StringP("channel", "c", "", "Current message channel for commands that operate on a channel")
	viper.BindPFlag("api_url", rootCmd.PersistentFlags().Lookup("api-url"))
	viper.BindPFlag("channel", rootCmd.PersistentFlags().Lookup("channel"))

	rootCmd.AddCommand(loginCmd())
	rootCmd.AddCommand(storeCmd())
	rootCmd.AddCommand(searchCmd())
	rootCmd.AddCommand(listCmd())
	rootCmd.AddCommand(msgCmd())
	rootCmd.AddCommand(acpCmd())

	cobra.OnInitialize(initConfig)

	if err := rootCmd.Execute(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func initConfig() {
	if cfgFile != "" {
		viper.SetConfigFile(cfgFile)
	} else {
		home, err := os.UserHomeDir()
		if err == nil {
			configDir := filepath.Join(home, ".config", "rust-rag")
			os.MkdirAll(configDir, 0755)
			viper.AddConfigPath(configDir)
			viper.SetConfigName("config")
			viper.SetConfigType("json")
		}
	}

	viper.AutomaticEnv()
	if err := viper.ReadInConfig(); err == nil {
		// fmt.Println("Using config file:", viper.ConfigFileUsed())
	}
}

func saveConfig() error {
	return viper.WriteConfig()
}

func loginCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "login",
		Short: "Authenticate with the RAG server using device code flow",
		RunE: func(cmd *cobra.Command, args []string) error {
			apiURL := viper.GetString("api_url")
			clientName, _ := os.Hostname()

			// 1. Request device code
			resp, err := http.Post(apiURL+"/auth/device/code", "application/json", strings.NewReader(fmt.Sprintf(`{"client_name":"%s"}`, clientName)))
			if err != nil {
				return err
			}
			defer resp.Body.Close()

			if resp.StatusCode != http.StatusOK {
				body, _ := io.ReadAll(resp.Body)
				return fmt.Errorf("failed to get device code: %s", string(body))
			}

			var codeResp struct {
				DeviceCode              string `json:"device_code"`
				UserCode                string `json:"user_code"`
				VerificationURI         string `json:"verification_uri"`
				VerificationURIComplete string `json:"verification_uri_complete"`
				ExpiresIn               int    `json:"expires_in"`
				Interval                int    `json:"interval"`
			}
			json.NewDecoder(resp.Body).Decode(&codeResp)

			fmt.Printf("Please visit: %s\n", codeResp.VerificationURIComplete)
			fmt.Printf("Or enter code %s at %s\n", codeResp.UserCode, codeResp.VerificationURI)
			fmt.Println("Waiting for authorization...")

			// 2. Poll for token
			interval := time.Duration(codeResp.Interval) * time.Second
			if interval == 0 {
				interval = 5 * time.Second
			}
			timeout := time.Now().Add(time.Duration(codeResp.ExpiresIn) * time.Second)

			for time.Now().Before(timeout) {
				time.Sleep(interval)

				tokenReqBody := fmt.Sprintf(`{"device_code":"%s"}`, codeResp.DeviceCode)
				resp, err := http.Post(apiURL+"/auth/device/token", "application/json", strings.NewReader(tokenReqBody))
				if err != nil {
					continue
				}
				defer resp.Body.Close()

				if resp.StatusCode == http.StatusOK {
					var tokenResp struct {
						AccessToken string `json:"access_token"`
					}
					json.NewDecoder(resp.Body).Decode(&tokenResp)

					viper.Set("access_token", tokenResp.AccessToken)
					if err := viper.WriteConfig(); err != nil {
						// Create file if it doesn't exist
						home, _ := os.UserHomeDir()
						configPath := filepath.Join(home, ".config", "rust-rag", "config.json")
						viper.SetConfigFile(configPath)
						viper.WriteConfigAs(configPath)
					}

					fmt.Println("Login successful!")
					return nil
				}

				var errResp struct {
					Error string `json:"error"`
				}
				json.NewDecoder(resp.Body).Decode(&errResp)

				if errResp.Error == "authorization_pending" {
					continue
				} else if errResp.Error == "slow_down" {
					interval += 2 * time.Second
				} else {
					return fmt.Errorf("authentication failed: %s", errResp.Error)
				}
			}

			return fmt.Errorf("authentication timed out")
		},
	}
}

func storeCmd() *cobra.Command {
	var sourceID string
	var metadataStr string

	cmd := &cobra.Command{
		Use:   "store [text]",
		Short: "Store text in RAG. If text is omitted, it reads from stdin.",
		RunE: func(cmd *cobra.Command, args []string) error {
			var text string
			if len(args) > 0 {
				text = strings.Join(args, " ")
			} else {
				data, err := io.ReadAll(os.Stdin)
				if err != nil {
					return err
				}
				text = string(data)
			}

			if text == "" {
				return fmt.Errorf("no text provided")
			}

			var metadata map[string]interface{}
			if metadataStr != "" {
				if err := json.Unmarshal([]byte(metadataStr), &metadata); err != nil {
					return fmt.Errorf("invalid metadata JSON: %v", err)
				}
			} else {
				metadata = make(map[string]interface{})
			}

			apiURL := viper.GetString("api_url")
			token := viper.GetString("access_token")

			reqBody, _ := json.Marshal(map[string]interface{}{
				"text":      text,
				"source_id": sourceID,
				"metadata":  metadata,
			})

			req, _ := http.NewRequest("POST", apiURL+"/api/store", bytes.NewBuffer(reqBody))
			req.Header.Set("Content-Type", "application/json")
			req.Header.Set("Authorization", "Bearer "+token)

			resp, err := http.DefaultClient.Do(req)
			if err != nil {
				return err
			}
			defer resp.Body.Close()

			if resp.StatusCode != http.StatusCreated && resp.StatusCode != http.StatusOK {
				body, _ := io.ReadAll(resp.Body)
				return fmt.Errorf("failed to store item: %s", string(body))
			}

			var storeResp map[string]interface{}
			json.NewDecoder(resp.Body).Decode(&storeResp)
			fmt.Printf("Stored item ID: %v\n", storeResp["id"])
			return nil
		},
	}

	cmd.Flags().StringVarP(&sourceID, "source", "s", "default", "Source ID for the entry")
	cmd.Flags().StringVarP(&metadataStr, "metadata", "m", "", "JSON metadata for the entry")

	return cmd
}

func searchCmd() *cobra.Command {
	var topK int
	var sourceID string

	cmd := &cobra.Command{
		Use:   "search <query>",
		Short: "Search for entries in RAG",
		Args:  cobra.MinimumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			query := strings.Join(args, " ")
			apiURL := viper.GetString("api_url")
			token := viper.GetString("access_token")

			reqData := map[string]interface{}{
				"query": query,
				"top_k": topK,
			}
			if sourceID != "" {
				reqData["source_id"] = sourceID
			}
			reqBody, _ := json.Marshal(reqData)

			req, _ := http.NewRequest("POST", apiURL+"/api/search", bytes.NewBuffer(reqBody))
			req.Header.Set("Content-Type", "application/json")
			req.Header.Set("Authorization", "Bearer "+token)

			resp, err := http.DefaultClient.Do(req)
			if err != nil {
				return err
			}
			defer resp.Body.Close()

			if resp.StatusCode != http.StatusOK {
				body, _ := io.ReadAll(resp.Body)
				return fmt.Errorf("search failed: %s", string(body))
			}

			var searchResp struct {
				Results []struct {
					ID       string                 `json:"id"`
					Text     string                 `json:"text"`
					Distance float32                `json:"distance"`
					Metadata map[string]interface{} `json:"metadata"`
				} `json:"results"`
			}
			json.NewDecoder(resp.Body).Decode(&searchResp)

			for i, hit := range searchResp.Results {
				fmt.Printf("[%d] ID: %s (dist: %.4f)\n", i+1, hit.ID, hit.Distance)
				fmt.Printf("%s\n\n", hit.Text)
			}

			return nil
		},
	}

	cmd.Flags().IntVarP(&topK, "limit", "k", 5, "Number of results to return")
	cmd.Flags().StringVarP(&sourceID, "source", "s", "", "Restrict search to source ID")

	return cmd
}

func listCmd() *cobra.Command {
	var sourceID string
	var limit int

	cmd := &cobra.Command{
		Use:   "list",
		Short: "List recent entries",
		RunE: func(cmd *cobra.Command, args []string) error {
			apiURL := viper.GetString("api_url")
			token := viper.GetString("access_token")

			url := fmt.Sprintf("%s/admin/items?limit=%d", apiURL, limit)
			if sourceID != "" {
				url += "&source_id=" + sourceID
			}

			req, _ := http.NewRequest("GET", url, nil)
			req.Header.Set("Authorization", "Bearer "+token)

			resp, err := http.DefaultClient.Do(req)
			if err != nil {
				return err
			}
			defer resp.Body.Close()

			if resp.StatusCode != http.StatusOK {
				body, _ := io.ReadAll(resp.Body)
				return fmt.Errorf("list failed: %s", string(body))
			}

			var listResp struct {
				Items []struct {
					ID        string `json:"id"`
					Text      string `json:"text"`
					CreatedAt int64  `json:"created_at"`
				} `json:"items"`
				TotalCount int64 `json:"total_count"`
			}
			json.NewDecoder(resp.Body).Decode(&listResp)

			fmt.Printf("Found %d items total. Showing %d most recent:\n\n", listResp.TotalCount, len(listResp.Items))
			for _, item := range listResp.Items {
				created := time.Unix(item.CreatedAt/1000, 0).Format(time.RFC3339)
				fmt.Printf("[%s] %s: %s\n", created, item.ID, truncate(item.Text, 100))
			}

			return nil
		},
	}

	cmd.Flags().StringVarP(&sourceID, "source", "s", "", "Filter by source ID")
	cmd.Flags().IntVarP(&limit, "limit", "n", 10, "Number of items to show")

	return cmd
}

func msgCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "msg",
		Short: "Send and read chat messages (slack-like channels)",
	}
	cmd.AddCommand(msgSendCmd())
	cmd.AddCommand(msgHistoryCmd())
	cmd.AddCommand(msgChannelsCmd())
	return cmd
}

func msgSendCmd() *cobra.Command {
	var sender, senderKind string

	cmd := &cobra.Command{
		Use:   "send [text]",
		Short: "Send a message to a channel. If text is omitted, reads from stdin.",
		RunE: func(cmd *cobra.Command, args []string) error {
			channel := currentChannel()
			if channel == "" {
				return fmt.Errorf("channel is required (set --channel)")
			}
			var text string
			if len(args) > 0 {
				text = strings.Join(args, " ")
			} else {
				data, err := io.ReadAll(os.Stdin)
				if err != nil {
					return err
				}
				text = strings.TrimSpace(string(data))
			}
			if text == "" {
				return fmt.Errorf("no text provided")
			}

			apiURL := viper.GetString("api_url")
			token := viper.GetString("access_token")

			body := map[string]interface{}{
				"channel": channel,
				"text":    text,
			}
			if sender != "" {
				body["sender"] = sender
			}
			if senderKind != "" {
				body["sender_kind"] = senderKind
			}
			reqBody, _ := json.Marshal(body)

			req, _ := http.NewRequest("POST", apiURL+"/api/messages", bytes.NewBuffer(reqBody))
			req.Header.Set("Content-Type", "application/json")
			if token != "" {
				req.Header.Set("Authorization", "Bearer "+token)
			}

			resp, err := http.DefaultClient.Do(req)
			if err != nil {
				return err
			}
			defer resp.Body.Close()

			if resp.StatusCode != http.StatusCreated && resp.StatusCode != http.StatusOK {
				b, _ := io.ReadAll(resp.Body)
				return fmt.Errorf("send failed: %s", string(b))
			}

			var msg struct {
				ID        string `json:"id"`
				Channel   string `json:"channel"`
				Sender    string `json:"sender"`
				CreatedAt int64  `json:"created_at"`
			}
			json.NewDecoder(resp.Body).Decode(&msg)
			fmt.Printf("Sent %s to #%s as %s\n", msg.ID, msg.Channel, msg.Sender)
			return nil
		},
	}

	cmd.Flags().StringVar(&sender, "sender", "", "Override sender label")
	cmd.Flags().StringVar(&senderKind, "kind", "", "Sender kind: human|agent|system")

	return cmd
}

func msgHistoryCmd() *cobra.Command {
	var sender, sortOrder string
	var since, until int64
	var limit int

	cmd := &cobra.Command{
		Use:   "history",
		Short: "Show message history with optional filters",
		RunE: func(cmd *cobra.Command, args []string) error {
			apiURL := viper.GetString("api_url")
			token := viper.GetString("access_token")
			channel := currentChannel()

			params := []string{}
			if channel != "" {
				params = append(params, "channel="+channel)
			}
			if sender != "" {
				params = append(params, "sender="+sender)
			}
			if since > 0 {
				params = append(params, fmt.Sprintf("since=%d", since))
			}
			if until > 0 {
				params = append(params, fmt.Sprintf("until=%d", until))
			}
			if limit > 0 {
				params = append(params, fmt.Sprintf("limit=%d", limit))
			}
			if sortOrder != "" {
				params = append(params, "sort_order="+sortOrder)
			}
			url := apiURL + "/api/messages"
			if len(params) > 0 {
				url += "?" + strings.Join(params, "&")
			}

			req, _ := http.NewRequest("GET", url, nil)
			if token != "" {
				req.Header.Set("Authorization", "Bearer "+token)
			}
			resp, err := http.DefaultClient.Do(req)
			if err != nil {
				return err
			}
			defer resp.Body.Close()

			if resp.StatusCode != http.StatusOK {
				b, _ := io.ReadAll(resp.Body)
				return fmt.Errorf("history failed: %s", string(b))
			}

			var listResp struct {
				Messages []struct {
					ID         string `json:"id"`
					Channel    string `json:"channel"`
					Sender     string `json:"sender"`
					SenderKind string `json:"sender_kind"`
					Text       string `json:"text"`
					CreatedAt  int64  `json:"created_at"`
				} `json:"messages"`
				TotalCount int64 `json:"total_count"`
			}
			json.NewDecoder(resp.Body).Decode(&listResp)

			fmt.Printf("%d of %d messages\n\n", len(listResp.Messages), listResp.TotalCount)
			for _, m := range listResp.Messages {
				ts := time.Unix(m.CreatedAt/1000, 0).Format("2006-01-02 15:04:05")
				fmt.Printf("[%s] #%s %s(%s): %s\n", ts, m.Channel, m.Sender, m.SenderKind, m.Text)
			}
			return nil
		},
	}

	cmd.Flags().StringVarP(&sender, "sender", "u", "", "Filter by sender")
	cmd.Flags().Int64Var(&since, "since", 0, "Min created_at (ms epoch)")
	cmd.Flags().Int64Var(&until, "until", 0, "Max created_at (ms epoch)")
	cmd.Flags().IntVarP(&limit, "limit", "n", 50, "Max messages")
	cmd.Flags().StringVar(&sortOrder, "sort", "desc", "asc | desc")

	return cmd
}

func msgChannelsCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "channels",
		Short: "List all message channels",
		RunE: func(cmd *cobra.Command, args []string) error {
			apiURL := viper.GetString("api_url")
			token := viper.GetString("access_token")

			req, _ := http.NewRequest("GET", apiURL+"/api/messages/channels", nil)
			if token != "" {
				req.Header.Set("Authorization", "Bearer "+token)
			}
			resp, err := http.DefaultClient.Do(req)
			if err != nil {
				return err
			}
			defer resp.Body.Close()
			if resp.StatusCode != http.StatusOK {
				b, _ := io.ReadAll(resp.Body)
				return fmt.Errorf("channels failed: %s", string(b))
			}
			var listResp struct {
				Channels []struct {
					Channel       string `json:"channel"`
					MessageCount  int64  `json:"message_count"`
					LastMessageAt int64  `json:"last_message_at"`
				} `json:"channels"`
			}
			json.NewDecoder(resp.Body).Decode(&listResp)

			for _, c := range listResp.Channels {
				ts := time.Unix(c.LastMessageAt/1000, 0).Format("2006-01-02 15:04")
				fmt.Printf("#%s  %d msgs  last: %s\n", c.Channel, c.MessageCount, ts)
			}
			return nil
		},
	}
}

func acpCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "acp",
		Short: "Bridge an ACP agent (gemini, copilot, ...) into a messaging channel",
	}
	cmd.AddCommand(acpRunCmd())
	cmd.AddCommand(acpGeminiCmd())
	cmd.AddCommand(acpCopilotCmd())
	cmd.AddCommand(acpBridgeCmd())
	return cmd
}

func acpRunCmd() *cobra.Command {
	var botName, command, cwd, root string
	var args []string
	var debug bool

	cmd := &cobra.Command{
		Use:   "run -- <command> [args...]",
		Short: "Spawn any ACP agent and bridge it to a messaging channel",
		Long: `Spawn an ACP-speaking agent process and bridge it to a rag messaging channel.

Example: rag acp run --channel ops --bot gemini-bot -- gemini --acp`,
		RunE: func(cmd *cobra.Command, posArgs []string) error {
			if command == "" {
				if len(posArgs) == 0 {
					return fmt.Errorf("missing agent command (use --command or pass after --)")
				}
				command = posArgs[0]
				args = posArgs[1:]
			} else {
				args = posArgs
			}
			return runAcpBridge(currentChannel(), botName, command, args, cwd, root, debug)
		},
	}
	cmd.Flags().StringVarP(&botName, "name", "n", "", "Bot/instance name. Used as sender label and as the @mention prefix that gates prompts. Defaults to agent name.")
	cmd.Flags().StringVar(&botName, "bot", "", "Alias for --name (deprecated)")
	cmd.Flags().StringVar(&command, "command", "", "Agent executable (alternative to passing after --)")
	cmd.Flags().StringVar(&cwd, "cwd", "", "Working directory for the agent (defaults to current dir)")
	cmd.Flags().StringVar(&root, "root", "", "Root folder to scan for projects")
	cmd.Flags().BoolVar(&debug, "debug", false, "Log raw JSON-RPC traffic")
	return cmd
}

func acpGeminiCmd() *cobra.Command {
	var botName, cwd, root string
	var debug bool
	cmd := &cobra.Command{
		Use:   "gemini",
		Short: "Bridge `gemini --acp` to a messaging channel",
		RunE: func(cmd *cobra.Command, _ []string) error {
			if botName == "" {
				botName = "gemini"
			}
			return runAcpBridge(currentChannel(), botName, "gemini", []string{"--acp"}, cwd, root, debug)
		},
	}
	cmd.Flags().StringVarP(&botName, "name", "n", "", "Bot/instance name + @mention prefix (defaults to 'gemini')")
	cmd.Flags().StringVar(&botName, "bot", "", "Alias for --name (deprecated)")
	cmd.Flags().StringVar(&cwd, "cwd", "", "Working directory (defaults to current dir)")
	cmd.Flags().StringVar(&root, "root", "", "Root folder to scan for projects")
	cmd.Flags().BoolVar(&debug, "debug", false, "Log raw JSON-RPC traffic")
	return cmd
}

func acpCopilotCmd() *cobra.Command {
	var botName, cwd, root string
	var debug bool
	cmd := &cobra.Command{
		Use:   "copilot",
		Short: "Bridge `copilot --acp` to a messaging channel (placeholder; adjust args to match your copilot binary)",
		RunE: func(cmd *cobra.Command, _ []string) error {
			if botName == "" {
				botName = "copilot"
			}
			return runAcpBridge(currentChannel(), botName, "copilot", []string{"--acp"}, cwd, root, debug)
		},
	}
	cmd.Flags().StringVarP(&botName, "name", "n", "", "Bot/instance name + @mention prefix (defaults to 'copilot')")
	cmd.Flags().StringVar(&botName, "bot", "", "Alias for --name (deprecated)")
	cmd.Flags().StringVar(&cwd, "cwd", "", "Working directory (defaults to current dir)")
	cmd.Flags().StringVar(&root, "root", "", "Root folder to scan for projects")
	cmd.Flags().BoolVar(&debug, "debug", false, "Log raw JSON-RPC traffic")
	return cmd
}

func runAcpBridge(channel, botName, command string, args []string, cwd, root string, debug bool) error {
	if channel == "" {
		return fmt.Errorf("channel is required (set --channel)")
	}
	if botName == "" {
		botName = filepath.Base(command)
	}
	if cwd == "" {
		cwd, _ = os.Getwd()
	}
	apiURL := viper.GetString("api_url")
	token := viper.GetString("access_token")

	bridge := acp.NewBridge(acp.BridgeConfig{
		APIURL:      apiURL,
		AccessToken: token,
		Channel:     channel,
		BotName:     botName,
		AgentName:   botName,
		Command:     command,
		Args:        args,
		CWD:         cwd,
		Root:        root,
		Debug:       debug,
	})

	return runBridge(bridge, command, channel, botName, apiURL)
}

func acpBridgeCmd() *cobra.Command {
	var botName, root string
	var agents []string
	var debug bool

	cmd := &cobra.Command{
		Use:   "bridge",
		Short: "Start a multi-agent ACP bridge",
		RunE: func(cmd *cobra.Command, _ []string) error {
			agentMap := make(map[string]acp.AgentSpec)
			for _, a := range agents {
				parts := strings.SplitN(a, "=", 2)
				if len(parts) != 2 {
					return fmt.Errorf("invalid agent spec: %s (expected name=command)", a)
				}
				cmdParts := strings.Fields(parts[1])
				agentMap[parts[0]] = acp.AgentSpec{
					Command: cmdParts[0],
					Args:    cmdParts[1:],
				}
			}
			return runAcpMultiBridge(currentChannel(), botName, agentMap, root, debug)
		},
	}
	cmd.Flags().StringVarP(&botName, "name", "n", "bridge", "Bot/instance name + @mention prefix")
	cmd.Flags().StringSliceVarP(&agents, "agent", "a", nil, "Available agents in name=command format")
	cmd.Flags().StringVar(&root, "root", "", "Root folder to scan for projects")
	cmd.Flags().BoolVar(&debug, "debug", false, "Log raw JSON-RPC traffic")
	return cmd
}

func runAcpMultiBridge(channel, botName string, agents map[string]acp.AgentSpec, root string, debug bool) error {
	if channel == "" {
		return fmt.Errorf("channel is required (set --channel)")
	}

	// If default name is used, enrich it with hostname and directory
	if botName == "bridge" {
		host, _ := os.Hostname()
		if host == "" {
			host = "local"
		}
		// Use parts of hostname if it's a FQDN
		host = strings.Split(host, ".")[0]

		dir, _ := os.Getwd()
		if root != "" {
			dir = root
		}
		folder := filepath.Base(dir)
		if folder == "." || folder == "/" {
			folder = "root"
		}
		botName = fmt.Sprintf("bridge-%s-%s", strings.ToLower(host), strings.ToLower(folder))
	}

	apiURL := viper.GetString("api_url")
	token := viper.GetString("access_token")

	bridge := acp.NewBridge(acp.BridgeConfig{
		APIURL:      apiURL,
		AccessToken: token,
		Channel:     channel,
		BotName:     botName,
		Agents:      agents,
		Root:        root,
		Debug:       debug,
	})

	return runBridge(bridge, "multi-agent bridge", channel, botName, apiURL)
}

func runBridge(bridge *acp.Bridge, desc, channel, botName, apiURL string) error {
	ctx, cancel := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer cancel()

	fmt.Printf("Bridging %s -> #%s as %s (api=%s)\n", desc, channel, botName, apiURL)
	if err := bridge.Run(ctx); err != nil {
		if ctx.Err() != nil {
			return nil
		}
		return err
	}
	return nil
}

func truncate(s string, max int) string {
	if len(s) > max {
		return s[:max] + "..."
	}
	return s
}

func currentChannel() string {
	if channel := strings.TrimSpace(viper.GetString("channel")); channel != "" {
		return channel
	}

	cwd, err := os.Getwd()
	if err != nil {
		return ""
	}

	return defaultChannelFromDir(cwd)
}

var nonChannelChars = regexp.MustCompile(`[^a-z0-9_-]+`)

func defaultChannelFromDir(dir string) string {
	base := strings.TrimSpace(filepath.Base(filepath.Clean(dir)))
	if base == "" || base == "." || base == string(filepath.Separator) {
		return ""
	}

	channel := strings.ToLower(base)
	channel = strings.ReplaceAll(channel, " ", "-")
	channel = nonChannelChars.ReplaceAllString(channel, "-")
	channel = strings.Trim(channel, "-")

	return channel
}
