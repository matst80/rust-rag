package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/spf13/cobra"
	"github.com/spf13/viper"
)

type Config struct {
	APIURL      string `json:"api_url"`
	AccessToken string `json:"access_token"`
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
	viper.BindPFlag("api_url", rootCmd.PersistentFlags().Lookup("api-url"))

	rootCmd.AddCommand(loginCmd())
	rootCmd.AddCommand(storeCmd())
	rootCmd.AddCommand(searchCmd())
	rootCmd.AddCommand(listCmd())

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

func truncate(s string, max int) string {
	if len(s) > max {
		return s[:max] + "..."
	}
	return s
}
