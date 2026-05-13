package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"mime/multipart"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/spf13/cobra"
	"github.com/spf13/viper"
)

var (
	cfgFile  string
	jsonOut  bool
)

func main() {
	if err := newRoot().Execute(); err != nil {
		fmt.Fprintln(os.Stderr, "error:", err)
		os.Exit(1)
	}
}

func newRoot() *cobra.Command {
	root := &cobra.Command{
		Use:   "rag",
		Short: "rust-rag CLI — entries, attachments, search, graph.",
	}

	root.PersistentFlags().StringVar(&cfgFile, "config", "", "config file (default $HOME/.config/rust-rag/config.json)")
	root.PersistentFlags().String("api-url", "http://localhost:4001", "rust-rag API base URL")
	root.PersistentFlags().BoolVar(&jsonOut, "json", false, "emit raw JSON responses")
	viper.BindPFlag("api_url", root.PersistentFlags().Lookup("api-url"))

	cobra.OnInitialize(initConfig)

	root.AddCommand(
		loginCmd(),
		entryCmd(),
		searchCmd(),
		ingestCmd(),
		sourcesCmd(),
		schemaCmd(),
		edgeCmd(),
		attachCmd(),
		dreamCmd(),
		healthCmd(),
	)
	return root
}

func initConfig() {
	if cfgFile != "" {
		viper.SetConfigFile(cfgFile)
	} else {
		home, err := os.UserHomeDir()
		if err == nil {
			dir := filepath.Join(home, ".config", "rust-rag")
			os.MkdirAll(dir, 0o755)
			viper.AddConfigPath(dir)
			viper.SetConfigName("config")
			viper.SetConfigType("json")
		}
	}
	viper.AutomaticEnv()
	_ = viper.ReadInConfig()
}

// ----- HTTP helpers -----

type apiClient struct {
	BaseURL string
	Token   string
}

func client() *apiClient {
	return &apiClient{
		BaseURL: strings.TrimRight(viper.GetString("api_url"), "/"),
		Token:   viper.GetString("access_token"),
	}
}

func (c *apiClient) do(method, path string, body, out any) error {
	var reader io.Reader
	if body != nil {
		b, err := json.Marshal(body)
		if err != nil {
			return err
		}
		reader = bytes.NewReader(b)
	}
	req, err := http.NewRequest(method, c.BaseURL+path, reader)
	if err != nil {
		return err
	}
	if body != nil {
		req.Header.Set("Content-Type", "application/json")
	}
	if c.Token != "" {
		req.Header.Set("Authorization", "Bearer "+c.Token)
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	respBody, _ := io.ReadAll(resp.Body)
	if resp.StatusCode >= 400 {
		return fmt.Errorf("%s %s -> %d: %s", method, path, resp.StatusCode, strings.TrimSpace(string(respBody)))
	}
	if jsonOut {
		os.Stdout.Write(respBody)
		if len(respBody) > 0 && respBody[len(respBody)-1] != '\n' {
			fmt.Println()
		}
		return nil
	}
	if out != nil && len(respBody) > 0 {
		return json.Unmarshal(respBody, out)
	}
	return nil
}

func (c *apiClient) postMultipart(path string, fields map[string]string, fileField, filePath string, out any) error {
	body := &bytes.Buffer{}
	mw := multipart.NewWriter(body)
	for k, v := range fields {
		_ = mw.WriteField(k, v)
	}
	if filePath != "" {
		f, err := os.Open(filePath)
		if err != nil {
			return err
		}
		defer f.Close()
		fw, err := mw.CreateFormFile(fileField, filepath.Base(filePath))
		if err != nil {
			return err
		}
		if _, err := io.Copy(fw, f); err != nil {
			return err
		}
	}
	mw.Close()

	req, err := http.NewRequest("POST", c.BaseURL+path, body)
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", mw.FormDataContentType())
	if c.Token != "" {
		req.Header.Set("Authorization", "Bearer "+c.Token)
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	respBody, _ := io.ReadAll(resp.Body)
	if resp.StatusCode >= 400 {
		return fmt.Errorf("POST %s -> %d: %s", path, resp.StatusCode, strings.TrimSpace(string(respBody)))
	}
	if jsonOut {
		os.Stdout.Write(respBody)
		if len(respBody) > 0 && respBody[len(respBody)-1] != '\n' {
			fmt.Println()
		}
		return nil
	}
	if out != nil && len(respBody) > 0 {
		return json.Unmarshal(respBody, out)
	}
	return nil
}

func readTextArg(args []string) (string, error) {
	if len(args) > 0 {
		return strings.Join(args, " "), nil
	}
	data, err := io.ReadAll(os.Stdin)
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(string(data)), nil
}

func parseJSON(s string) (any, error) {
	if s == "" {
		return nil, nil
	}
	var v any
	if err := json.Unmarshal([]byte(s), &v); err != nil {
		return nil, fmt.Errorf("invalid JSON: %w", err)
	}
	return v, nil
}

// ----- login -----

func loginCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "login",
		Short: "Authenticate via device code flow",
		RunE: func(cmd *cobra.Command, args []string) error {
			c := client()
			host, _ := os.Hostname()
			resp, err := http.Post(c.BaseURL+"/auth/device/code", "application/json",
				strings.NewReader(fmt.Sprintf(`{"client_name":%q}`, host)))
			if err != nil {
				return err
			}
			defer resp.Body.Close()
			if resp.StatusCode != http.StatusOK {
				b, _ := io.ReadAll(resp.Body)
				return fmt.Errorf("device code: %s", string(b))
			}
			var code struct {
				DeviceCode              string `json:"device_code"`
				UserCode                string `json:"user_code"`
				VerificationURI         string `json:"verification_uri"`
				VerificationURIComplete string `json:"verification_uri_complete"`
				ExpiresIn               int    `json:"expires_in"`
				Interval                int    `json:"interval"`
			}
			json.NewDecoder(resp.Body).Decode(&code)
			fmt.Printf("Visit: %s\n", code.VerificationURIComplete)
			fmt.Printf("Or enter code %s at %s\n", code.UserCode, code.VerificationURI)

			interval := time.Duration(code.Interval) * time.Second
			if interval == 0 {
				interval = 5 * time.Second
			}
			deadline := time.Now().Add(time.Duration(code.ExpiresIn) * time.Second)
			for time.Now().Before(deadline) {
				time.Sleep(interval)
				tr, err := http.Post(c.BaseURL+"/auth/device/token", "application/json",
					strings.NewReader(fmt.Sprintf(`{"device_code":%q}`, code.DeviceCode)))
				if err != nil {
					continue
				}
				if tr.StatusCode == http.StatusOK {
					var tok struct{ AccessToken string `json:"access_token"` }
					json.NewDecoder(tr.Body).Decode(&tok)
					tr.Body.Close()
					viper.Set("access_token", tok.AccessToken)
					if err := viper.WriteConfig(); err != nil {
						home, _ := os.UserHomeDir()
						p := filepath.Join(home, ".config", "rust-rag", "config.json")
						viper.SetConfigFile(p)
						viper.WriteConfigAs(p)
					}
					fmt.Println("Login successful.")
					return nil
				}
				var er struct{ Error string `json:"error"` }
				json.NewDecoder(tr.Body).Decode(&er)
				tr.Body.Close()
				switch er.Error {
				case "authorization_pending":
				case "slow_down":
					interval += 2 * time.Second
				default:
					return fmt.Errorf("auth failed: %s", er.Error)
				}
			}
			return fmt.Errorf("auth timed out")
		},
	}
}

// ----- entry -----

func entryCmd() *cobra.Command {
	cmd := &cobra.Command{Use: "entry", Short: "Manage entries"}
	cmd.AddCommand(
		entryStoreCmd(),
		entrySmartCmd(),
		entryGetCmd(),
		entryUpdateCmd(),
		entryDeleteCmd(),
		entryListCmd(),
		entryBrowseCmd(),
		entryRelatedCmd(),
		entryAnalyzeCmd(),
		entryImageCmd(),
	)
	return cmd
}

func entryStoreCmd() *cobra.Command {
	var sourceID, metaStr, typeName, dataStr, path, title string
	var tags []string

	cmd := &cobra.Command{
		Use:   "store [text]",
		Short: "Store a new entry (text from arg or stdin)",
		RunE: func(cmd *cobra.Command, args []string) error {
			text, err := readTextArg(args)
			if err != nil {
				return err
			}
			if text == "" {
				return fmt.Errorf("no text provided")
			}
			meta := map[string]any{}
			if metaStr != "" {
				if err := json.Unmarshal([]byte(metaStr), &meta); err != nil {
					return fmt.Errorf("invalid metadata: %w", err)
				}
			}
			if title != "" {
				meta["title"] = title
			}
			if len(tags) > 0 {
				meta["tags"] = tags
			}
			body := map[string]any{
				"text":      text,
				"source_id": sourceID,
				"metadata":  meta,
			}
			if path != "" {
				body["path"] = path
			}
			if typeName != "" {
				body["type"] = typeName
			}
			if dataStr != "" {
				v, err := parseJSON(dataStr)
				if err != nil {
					return err
				}
				body["data"] = v
			}
			var resp struct {
				ID        string `json:"id"`
				CreatedAt int64  `json:"created_at"`
			}
			if err := client().do("POST", "/api/store", body, &resp); err != nil {
				return err
			}
			if !jsonOut {
				fmt.Printf("stored: %s\n", resp.ID)
			}
			return nil
		},
	}
	cmd.Flags().StringVarP(&sourceID, "source", "s", "default", "source_id namespace")
	cmd.Flags().StringVarP(&metaStr, "metadata", "m", "", "metadata JSON")
	cmd.Flags().StringVar(&typeName, "type", "", "typed-entry schema name")
	cmd.Flags().StringVar(&dataStr, "data", "", "typed payload JSON")
	cmd.Flags().StringVar(&path, "path", "", "wiki path (e.g. team/handbook)")
	cmd.Flags().StringVar(&title, "title", "", "title (stored in metadata.title)")
	cmd.Flags().StringSliceVar(&tags, "tags", nil, "metadata.tags entries")
	return cmd
}

func entrySmartCmd() *cobra.Command {
	var urlCtx, titleCtx, model string

	cmd := &cobra.Command{
		Use:   "smart [text]",
		Short: "LLM-assisted store: split text into multiple entries with auto source_id/metadata",
		RunE: func(cmd *cobra.Command, args []string) error {
			text, err := readTextArg(args)
			if err != nil {
				return err
			}
			if text == "" {
				return fmt.Errorf("no text provided")
			}
			body := map[string]any{"text": text}
			if urlCtx != "" || titleCtx != "" {
				ctx := map[string]any{}
				if urlCtx != "" {
					ctx["url"] = urlCtx
				}
				if titleCtx != "" {
					ctx["title"] = titleCtx
				}
				body["context"] = ctx
			}
			if model != "" {
				body["model"] = model
			}
			var resp struct {
				Items []struct {
					ID       string `json:"id"`
					SourceID string `json:"source_id"`
				} `json:"items"`
			}
			if err := client().do("POST", "/api/store/smart", body, &resp); err != nil {
				return err
			}
			if !jsonOut {
				for _, it := range resp.Items {
					fmt.Printf("stored: %s (src=%s)\n", it.ID, it.SourceID)
				}
			}
			return nil
		},
	}
	cmd.Flags().StringVar(&urlCtx, "url", "", "context: source URL")
	cmd.Flags().StringVar(&titleCtx, "title", "", "context: page title")
	cmd.Flags().StringVar(&model, "model", "", "override LLM model")
	return cmd
}

func entryGetCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "get <id>",
		Short: "Get a single entry",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			var item map[string]any
			if err := client().do("GET", "/admin/items/"+url.PathEscape(args[0]), nil, &item); err != nil {
				return err
			}
			if !jsonOut {
				printItem(item)
			}
			return nil
		},
	}
}

func entryUpdateCmd() *cobra.Command {
	var text, metaStr, sourceID, path, typeName, dataStr string

	cmd := &cobra.Command{
		Use:   "update <id>",
		Short: "Update an entry (any subset of fields)",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			id := args[0]
			var current map[string]any
			if err := client().do("GET", "/admin/items/"+url.PathEscape(id), nil, &current); err != nil {
				return err
			}
			body := map[string]any{
				"text":      current["text"],
				"metadata":  current["metadata"],
				"source_id": current["source_id"],
			}
			if v, ok := current["path"]; ok && v != nil {
				body["path"] = v
			}
			if v, ok := current["type"]; ok && v != nil {
				body["type"] = v
			}
			if v, ok := current["data"]; ok && v != nil {
				body["data"] = v
			}

			if text != "" {
				body["text"] = text
			}
			if sourceID != "" {
				body["source_id"] = sourceID
			}
			if path != "" {
				body["path"] = path
			}
			if typeName != "" {
				body["type"] = typeName
			}
			if metaStr != "" {
				v, err := parseJSON(metaStr)
				if err != nil {
					return err
				}
				body["metadata"] = v
			}
			if dataStr != "" {
				v, err := parseJSON(dataStr)
				if err != nil {
					return err
				}
				body["data"] = v
			}
			if err := client().do("PUT", "/admin/items/"+url.PathEscape(id), body, nil); err != nil {
				return err
			}
			if !jsonOut {
				fmt.Printf("updated: %s\n", id)
			}
			return nil
		},
	}
	cmd.Flags().StringVar(&text, "text", "", "new text")
	cmd.Flags().StringVar(&metaStr, "metadata", "", "metadata JSON (replaces)")
	cmd.Flags().StringVar(&sourceID, "source", "", "source_id")
	cmd.Flags().StringVar(&path, "path", "", "wiki path")
	cmd.Flags().StringVar(&typeName, "type", "", "type schema name")
	cmd.Flags().StringVar(&dataStr, "data", "", "typed payload JSON")
	return cmd
}

func entryDeleteCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "delete <id>",
		Short: "Delete an entry",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			id := args[0]
			if err := client().do("DELETE", "/admin/items/"+url.PathEscape(id), nil, nil); err != nil {
				return err
			}
			if !jsonOut {
				fmt.Printf("deleted: %s\n", id)
			}
			return nil
		},
	}
}

func entryListCmd() *cobra.Command {
	var sourceID, pathPrefix, typeName string
	var limit int

	cmd := &cobra.Command{
		Use:   "list",
		Short: "List entries",
		RunE: func(cmd *cobra.Command, args []string) error {
			q := url.Values{}
			if sourceID != "" {
				q.Set("source_id", sourceID)
			}
			if pathPrefix != "" {
				q.Set("path_prefix", pathPrefix)
			}
			if typeName != "" {
				q.Set("type", typeName)
			}
			q.Set("limit", fmt.Sprintf("%d", limit))
			var resp struct {
				Items []map[string]any `json:"items"`
				TotalCount int64 `json:"total_count"`
			}
			if err := client().do("GET", "/admin/items?"+q.Encode(), nil, &resp); err != nil {
				return err
			}
			if !jsonOut {
				fmt.Printf("%d of %d:\n\n", len(resp.Items), resp.TotalCount)
				for _, it := range resp.Items {
					printItemRow(it)
				}
			}
			return nil
		},
	}
	cmd.Flags().StringVarP(&sourceID, "source", "s", "", "filter by source_id")
	cmd.Flags().StringVar(&pathPrefix, "path-prefix", "", "filter by path prefix")
	cmd.Flags().StringVar(&typeName, "type", "", "filter by typed-entry schema")
	cmd.Flags().IntVarP(&limit, "limit", "n", 20, "result limit")
	return cmd
}

func entryBrowseCmd() *cobra.Command {
	var sourceID, prefix string

	cmd := &cobra.Command{
		Use:   "browse",
		Short: "Browse entries by wiki tree path",
		RunE: func(cmd *cobra.Command, args []string) error {
			q := url.Values{}
			if sourceID != "" {
				q.Set("source_id", sourceID)
			}
			if prefix != "" {
				q.Set("prefix", prefix)
			}
			var resp any
			if err := client().do("GET", "/api/entries/tree?"+q.Encode(), nil, &resp); err != nil {
				return err
			}
			if !jsonOut {
				b, _ := json.MarshalIndent(resp, "", "  ")
				fmt.Println(string(b))
			}
			return nil
		},
	}
	cmd.Flags().StringVarP(&sourceID, "source", "s", "", "source_id")
	cmd.Flags().StringVar(&prefix, "prefix", "", "wiki path prefix")
	return cmd
}

func entryRelatedCmd() *cobra.Command {
	var depth, limit int
	var edgeType string

	cmd := &cobra.Command{
		Use:   "related <id>",
		Short: "Show graph neighborhood of an entry",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			q := url.Values{}
			if depth > 0 {
				q.Set("depth", fmt.Sprintf("%d", depth))
			}
			if limit > 0 {
				q.Set("limit", fmt.Sprintf("%d", limit))
			}
			if edgeType != "" {
				q.Set("edge_type", edgeType)
			}
			path := "/api/graph/neighborhood/" + url.PathEscape(args[0])
			if e := q.Encode(); e != "" {
				path += "?" + e
			}
			var resp struct {
				Nodes []map[string]any `json:"nodes"`
				Edges []map[string]any `json:"edges"`
			}
			if err := client().do("GET", path, nil, &resp); err != nil {
				return err
			}
			if !jsonOut {
				fmt.Printf("nodes: %d  edges: %d\n\n", len(resp.Nodes), len(resp.Edges))
				for _, n := range resp.Nodes {
					printItemRow(n)
				}
			}
			return nil
		},
	}
	cmd.Flags().IntVar(&depth, "depth", 1, "graph depth")
	cmd.Flags().IntVar(&limit, "limit", 20, "node limit")
	cmd.Flags().StringVar(&edgeType, "edge-type", "", "filter edge type (manual|similarity)")
	return cmd
}

func entryAnalyzeCmd() *cobra.Command {
	var sourceID, excludeID string

	cmd := &cobra.Command{
		Use:   "analyze <id|->",
		Short: "Run LLM analysis on an existing entry id, or on text from stdin (use -)",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			c := client()
			var text string
			if args[0] == "-" {
				b, err := io.ReadAll(os.Stdin)
				if err != nil {
					return err
				}
				text = strings.TrimSpace(string(b))
				if excludeID == "" {
					excludeID = ""
				}
			} else {
				var item struct {
					Text     string `json:"text"`
					SourceID string `json:"source_id"`
				}
				if err := c.do("GET", "/admin/items/"+url.PathEscape(args[0]), nil, &item); err != nil {
					return err
				}
				text = item.Text
				if sourceID == "" {
					sourceID = item.SourceID
				}
				if excludeID == "" {
					excludeID = args[0]
				}
			}
			body := map[string]any{"text": text}
			if sourceID != "" {
				body["source_id"] = sourceID
			}
			if excludeID != "" {
				body["exclude_id"] = excludeID
			}
			var resp any
			if err := c.do("POST", "/api/store/analyze", body, &resp); err != nil {
				return err
			}
			if !jsonOut {
				b, _ := json.MarshalIndent(resp, "", "  ")
				fmt.Println(string(b))
			}
			return nil
		},
	}
	cmd.Flags().StringVarP(&sourceID, "source", "s", "", "namespace for neighbor search")
	cmd.Flags().StringVar(&excludeID, "exclude", "", "id to exclude from neighbor search")
	return cmd
}

func entryImageCmd() *cobra.Command {
	var sourceID, metaStr string

	cmd := &cobra.Command{
		Use:   "image <path>",
		Short: "Ingest an image with automatic analysis",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			fields := map[string]string{}
			if sourceID != "" {
				fields["source_id"] = sourceID
			}
			if metaStr != "" {
				fields["metadata"] = metaStr
			}
			var resp struct{ ID string `json:"id"` }
			if err := client().postMultipart("/api/ingest/image", fields, "file", args[0], &resp); err != nil {
				return err
			}
			if !jsonOut {
				fmt.Printf("stored: %s\n", resp.ID)
			}
			return nil
		},
	}
	cmd.Flags().StringVarP(&sourceID, "source", "s", "images", "source_id")
	cmd.Flags().StringVarP(&metaStr, "metadata", "m", "", "metadata JSON")
	return cmd
}

// ----- search -----

func searchCmd() *cobra.Command {
	var topK int
	var sourceID, typeName string
	var rerank, noRerank, showRelated bool

	cmd := &cobra.Command{
		Use:   "search <query>",
		Short: "Semantic search across entries",
		Args:  cobra.MinimumNArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			body := map[string]any{
				"query": strings.Join(args, " "),
				"top_k": topK,
			}
			if sourceID != "" {
				body["source_id"] = sourceID
			}
			if typeName != "" {
				body["type"] = typeName
			}
			switch {
			case rerank:
				body["rerank"] = true
			case noRerank:
				body["rerank"] = false
			}
			var resp struct {
				Results []struct {
					ID       string  `json:"id"`
					Text     string  `json:"text"`
					Distance float32 `json:"distance"`
					SourceID string  `json:"source_id"`
				} `json:"results"`
				Related []struct {
					ID       string `json:"id"`
					Relation string `json:"relation"`
				} `json:"related"`
			}
			if err := client().do("POST", "/api/search", body, &resp); err != nil {
				return err
			}
			if !jsonOut {
				for i, h := range resp.Results {
					fmt.Printf("[%d] %s (dist=%.4f, src=%s)\n%s\n\n",
						i+1, h.ID, h.Distance, h.SourceID, truncate(h.Text, 300))
				}
				if showRelated && len(resp.Related) > 0 {
					fmt.Println("Related:")
					for _, r := range resp.Related {
						fmt.Printf("  - %s (%s)\n", r.ID, r.Relation)
					}
				}
			}
			return nil
		},
	}
	cmd.Flags().IntVarP(&topK, "limit", "k", 5, "top_k")
	cmd.Flags().StringVarP(&sourceID, "source", "s", "", "filter by source_id")
	cmd.Flags().StringVar(&typeName, "type", "", "filter by typed-entry schema")
	cmd.Flags().BoolVar(&rerank, "rerank", false, "force reranking on")
	cmd.Flags().BoolVar(&noRerank, "no-rerank", false, "force reranking off")
	cmd.Flags().BoolVar(&showRelated, "show-related", false, "print related entries")
	return cmd
}

// ----- ingest (URL) -----

func ingestCmd() *cobra.Command {
	var sourceID, path, typeName string
	var useCDP, llmClean bool

	cmd := &cobra.Command{
		Use:   "ingest <url>",
		Short: "Ingest a URL (fetch, clean, store)",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			body := map[string]any{
				"url":       args[0],
				"source_id": sourceID,
				"use_cdp":   useCDP,
				"llm_clean": llmClean,
			}
			if path != "" {
				body["path"] = path
			}
			if typeName != "" {
				body["type"] = typeName
			}
			var resp struct{ ID string `json:"id"` }
			if err := client().do("POST", "/api/ingest/url", body, &resp); err != nil {
				return err
			}
			if !jsonOut {
				fmt.Printf("stored: %s\n", resp.ID)
			}
			return nil
		},
	}
	cmd.Flags().StringVarP(&sourceID, "source", "s", "web", "source_id")
	cmd.Flags().StringVar(&path, "path", "", "wiki path")
	cmd.Flags().StringVar(&typeName, "type", "", "typed-entry schema name")
	cmd.Flags().BoolVar(&useCDP, "cdp", false, "fetch via remote Chrome DevTools (RAG_CDP_URL)")
	cmd.Flags().BoolVar(&llmClean, "llm-clean", false, "LLM-extract main content")
	return cmd
}

// ----- sources -----

func sourcesCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "sources",
		Short: "List source_id namespaces and item counts",
		RunE: func(cmd *cobra.Command, args []string) error {
			var resp struct {
				Categories []struct {
					SourceID  string `json:"source_id"`
					ItemCount int64  `json:"item_count"`
				} `json:"categories"`
			}
			if err := client().do("GET", "/admin/categories", nil, &resp); err != nil {
				return err
			}
			if !jsonOut {
				for _, c := range resp.Categories {
					fmt.Printf("%6d  %s\n", c.ItemCount, c.SourceID)
				}
			}
			return nil
		},
	}
}

// ----- schema -----

func schemaCmd() *cobra.Command {
	cmd := &cobra.Command{Use: "schema", Short: "Inspect typed-entry schemas"}
	cmd.AddCommand(
		&cobra.Command{
			Use:   "list",
			Short: "List registered typed-entry schemas",
			RunE: func(cmd *cobra.Command, args []string) error {
				var resp any
				if err := client().do("GET", "/api/schemas", nil, &resp); err != nil {
					return err
				}
				if !jsonOut {
					b, _ := json.MarshalIndent(resp, "", "  ")
					fmt.Println(string(b))
				}
				return nil
			},
		},
		&cobra.Command{
			Use:   "get <type>",
			Short: "Show a single schema",
			Args:  cobra.ExactArgs(1),
			RunE: func(cmd *cobra.Command, args []string) error {
				var resp any
				if err := client().do("GET", "/api/schemas/"+url.PathEscape(args[0]), nil, &resp); err != nil {
					return err
				}
				if !jsonOut {
					b, _ := json.MarshalIndent(resp, "", "  ")
					fmt.Println(string(b))
				}
				return nil
			},
		},
	)
	return cmd
}

// ----- edge -----

func edgeCmd() *cobra.Command {
	cmd := &cobra.Command{Use: "edge", Short: "Graph edges"}

	listCmd := &cobra.Command{
		Use:   "list",
		Short: "List graph edges",
		RunE: func(cmd *cobra.Command, args []string) error {
			q := url.Values{}
			if v, _ := cmd.Flags().GetString("item"); v != "" {
				q.Set("item_id", v)
			}
			if v, _ := cmd.Flags().GetString("type"); v != "" {
				q.Set("edge_type", v)
			}
			var resp struct{ Edges []map[string]any `json:"edges"` }
			path := "/api/graph/edges"
			if e := q.Encode(); e != "" {
				path += "?" + e
			}
			if err := client().do("GET", path, nil, &resp); err != nil {
				return err
			}
			if !jsonOut {
				for _, e := range resp.Edges {
					fmt.Printf("%v -[%v/%v]-> %v\n",
						e["from_item_id"], e["edge_type"], e["relation"], e["to_item_id"])
				}
			}
			return nil
		},
	}
	listCmd.Flags().String("item", "", "filter by item_id (either side)")
	listCmd.Flags().String("type", "", "filter by edge_type (manual|similarity)")

	addCmd := &cobra.Command{
		Use:   "add <from> <to> <predicate>",
		Short: "Create a manual edge",
		Args:  cobra.ExactArgs(3),
		RunE: func(cmd *cobra.Command, args []string) error {
			weight, _ := cmd.Flags().GetFloat32("weight")
			directed, _ := cmd.Flags().GetBool("directed")
			body := map[string]any{
				"from_item_id": args[0],
				"to_item_id":   args[1],
				"relation":     args[2],
				"weight":       weight,
				"directed":     directed,
				"metadata":     map[string]any{},
			}
			var resp struct{ ID string `json:"id"` }
			if err := client().do("POST", "/admin/graph/edges", body, &resp); err != nil {
				return err
			}
			if !jsonOut {
				fmt.Printf("edge: %s\n", resp.ID)
			}
			return nil
		},
	}
	addCmd.Flags().Float32("weight", 1.0, "edge weight")
	addCmd.Flags().Bool("directed", false, "directed edge")

	cmd.AddCommand(listCmd, addCmd)
	return cmd
}

// ----- attachments -----

func attachCmd() *cobra.Command {
	cmd := &cobra.Command{Use: "attach", Short: "Manage attachments"}

	addCmd := &cobra.Command{
		Use:   "add <entry_id> <url>",
		Short: "Attach a remote URL to an entry",
		Args:  cobra.ExactArgs(2),
		RunE: func(cmd *cobra.Command, args []string) error {
			body := map[string]any{
				"item_id": args[0],
				"url":     args[1],
			}
			var resp struct{ ID string `json:"id"` }
			if err := client().do("POST", "/api/attachments/from-url", body, &resp); err != nil {
				return err
			}
			if !jsonOut {
				fmt.Printf("attachment: %s\n", resp.ID)
			}
			return nil
		},
	}

	listCmd := &cobra.Command{
		Use:   "list <entry_id>",
		Short: "List attachments for an entry",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			var resp any
			if err := client().do("GET", "/api/items/"+url.PathEscape(args[0])+"/attachments", nil, &resp); err != nil {
				return err
			}
			if !jsonOut {
				b, _ := json.MarshalIndent(resp, "", "  ")
				fmt.Println(string(b))
			}
			return nil
		},
	}

	rmCmd := &cobra.Command{
		Use:   "rm <attachment_id>",
		Short: "Delete an attachment",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if err := client().do("DELETE", "/api/attachments/"+url.PathEscape(args[0]), nil, nil); err != nil {
				return err
			}
			if !jsonOut {
				fmt.Printf("deleted: %s\n", args[0])
			}
			return nil
		},
	}

	cmd.AddCommand(addCmd, listCmd, rmCmd)
	return cmd
}

// ----- dream -----

func dreamCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "dream",
		Short: "Trigger a dreaming round (consolidation / linking)",
		RunE: func(cmd *cobra.Command, args []string) error {
			if err := client().do("POST", "/api/dream", nil, nil); err != nil {
				return err
			}
			if !jsonOut {
				fmt.Println("dreaming round triggered")
			}
			return nil
		},
	}
}

// ----- health -----

func healthCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "health",
		Short: "Server health",
		RunE: func(cmd *cobra.Command, args []string) error {
			var resp map[string]any
			if err := client().do("GET", "/healthz", nil, &resp); err != nil {
				return err
			}
			if !jsonOut {
				b, _ := json.MarshalIndent(resp, "", "  ")
				fmt.Println(string(b))
			}
			return nil
		},
	}
}

// ----- formatting -----

func truncate(s string, n int) string {
	s = strings.ReplaceAll(s, "\n", " ")
	if len(s) > n {
		return s[:n] + "..."
	}
	return s
}

func printItemRow(it map[string]any) {
	id, _ := it["id"].(string)
	text, _ := it["text"].(string)
	src, _ := it["source_id"].(string)
	created := ""
	if v, ok := it["created_at"].(float64); ok {
		created = time.Unix(int64(v)/1000, 0).Format("2006-01-02 15:04")
	}
	fmt.Printf("[%s] %s (%s): %s\n", created, id, src, truncate(text, 100))
}

func printItem(it map[string]any) {
	id, _ := it["id"].(string)
	src, _ := it["source_id"].(string)
	text, _ := it["text"].(string)
	fmt.Printf("id:        %s\n", id)
	fmt.Printf("source_id: %s\n", src)
	if v, ok := it["path"].(string); ok && v != "" {
		fmt.Printf("path:      %s\n", v)
	}
	if v, ok := it["type"].(string); ok && v != "" {
		fmt.Printf("type:      %s\n", v)
	}
	if v, ok := it["created_at"].(float64); ok {
		fmt.Printf("created:   %s\n", time.Unix(int64(v)/1000, 0).Format(time.RFC3339))
	}
	fmt.Println("---")
	fmt.Println(text)
}
