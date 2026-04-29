import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import path from "path";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
  ListPromptsRequestSchema,
  GetPromptRequestSchema,
  CreateMessageRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";

const BASE_URL = process.env.RAG_BASE_URL || "https://rag.k6n.net";
const API_KEY = process.env.RAG_API_KEY || process.env.RAG_MCP_AUTH_BEARER;

const server = new Server(
  {
    name: "gemini-chat-server",
    version: "1.1.1",
  },
  {
    capabilities: {
      tools: {},
      prompts: {},
      sampling: {},
    },
  }
);

let logs = [];

function log(message) {
  logs.push({
    timestamp: new Date().toISOString(),
    message: message,
  });
  console.error(message);
}

let lastSeenTimestamp = Date.now();
let lastMentionTimestamp = Date.now();
const DIR_NAME = path.basename(process.cwd());
const MENTION_PREFIX = `@${DIR_NAME}`;

async function apiRequest(endpoint, options = {}) {
  const url = `${BASE_URL}${endpoint}`;
  const headers = {
    "Content-Type": "application/json",
    ...options.headers,
  };

  if (API_KEY) {
    headers["Authorization"] = `Bearer ${API_KEY}`;
  }

  const response = await fetch(url, {
    ...options,
    headers,
  });

  if (!response.ok) {
    const errorBody = await response.text().catch(() => "Unknown error");
    throw new Error(`API error (${response.status}): ${errorBody}`);
  }

  return response.json();
}

async function sendSystemMessage(text, channel = "general") {
  console.error(`[SystemMessage] Sending to #${channel}: "${text}"`);
  try {
    const result = await apiRequest("/api/messages", {
      method: "POST",
      body: JSON.stringify({
        channel: channel,
        text: text,
        sender: "System",
        sender_kind: "system",
      }),
    });
    console.error(`[SystemMessage] Successfully sent: ${result.id}`);
    return result;
  } catch (err) {
    console.error(`[SystemMessage] Failed to send: ${err.message}`);
  }
}

server.setRequestHandler(ListToolsRequestSchema, async () => {
  return {
    tools: [
      {
        name: "send_message",
        description: "Send a message to the shared communication platform",
        inputSchema: {
          type: "object",
          properties: {
            text: {
              type: "string",
              description: "The message text to send",
            },
            channel: {
              type: "string",
              description: "The channel to send the message to (e.g., 'general', 'ops')",
              default: "general",
            },
            sender: {
              type: "string",
              description: "The sender name (defaults to 'Gemini')",
            },
          },
          required: ["text"],
        },
      },
      {
        name: "receive_messages",
        description: "Receive messages from the shared communication platform",
        inputSchema: {
          type: "object",
          properties: {
            channel: {
              type: "string",
              description: "The channel to fetch messages from",
              default: "general",
            },
            limit: {
              type: "number",
              description: "Maximum number of messages to fetch",
              default: 50,
            },
          },
        },
      },
      {
        name: "list_channels",
        description: "List available message channels",
        inputSchema: {
          type: "object",
          properties: {},
        },
      },
      {
        name: "debug_info",
        description: "Get debug information about the server state (polling status, timestamps, etc.)",
        inputSchema: {
          type: "object",
          properties: {},
        },
      },
    ],
  };
});

server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const { name, arguments: args } = request.params;

  if (name === "send_message") {
    const result = await apiRequest("/api/messages", {
      method: "POST",
      body: JSON.stringify({
        channel: args.channel || "general",
        text: args.text,
        sender: args.sender || "Gemini",
        sender_kind: args.sender_kind || "agent",
      }),
    });

    return {
      content: [{ type: "text", text: `Message sent to #${result.channel} (id: ${result.id})` }],
    };
  }


  if (name === "receive_messages") {
    const channel = args.channel || "general";
    const limit = args.limit || 50;
    const result = await apiRequest(`/api/messages?channel=${encodeURIComponent(channel)}&limit=${limit}&sort_order=asc`);

    if (result.messages.length === 0) {
      return {
        content: [{ type: "text", text: `No messages found in #${channel}.` }],
      };
    }

    const formattedMessages = result.messages
      .map((m) => `[${new Date(m.created_at).toISOString()}] ${m.sender} (${m.sender_kind})${m.text.startsWith(MENTION_PREFIX) ? ' [MENTION]' : ''}: ${m.text}`)
      .join("\n");

    return {
      content: [{ type: "text", text: `Messages in #${channel}:\n${formattedMessages}` }],
    };
  }

  if (name === "list_channels") {
    const result = await apiRequest("/api/messages/channels");
    const formattedChannels = result.channels
      .map((c) => `#${c.channel} (${c.message_count} messages, last: ${new Date(c.last_message_at).toISOString()})`)
      .join("\n");
    return {
      content: [{ type: "text", text: `Available channels:\n${formattedChannels}` }],
    };
  }

  if (name === "debug_info") {
    return {
      content: [{
        type: "text",
        text: JSON.stringify({
          logs,
          name: "gemini-chat-server",
          baseUrl: BASE_URL,
          dirName: DIR_NAME,
          mentionPrefix: MENTION_PREFIX,
          lastSeenTimestamp: new Date(lastSeenTimestamp).toISOString(),
          lastMentionTimestamp: new Date(lastMentionTimestamp).toISOString(),
          hasApiKey: !!API_KEY,
        }, null, 2)
      }],
    };
  }

  throw new Error(`Tool not found: ${name}`);
});

server.setRequestHandler(ListPromptsRequestSchema, async () => {
  return {
    prompts: [
      {
        name: "latest_messages",
        description: "Get the latest messages from the communication platform to respond to them",
      }
    ]
  };
});

server.setRequestHandler(GetPromptRequestSchema, async (request) => {
  if (request.params.name === "latest_messages") {
    const channel = "general";
    const result = await apiRequest(`/api/messages?channel=${encodeURIComponent(channel)}&limit=10&sort_order=desc`);
    const messages = result.messages.reverse();
    const formatted = messages.map(m => {
      const isMention = m.text.startsWith(MENTION_PREFIX);
      const text = isMention ? m.text.replace(MENTION_PREFIX, "").trim() : m.text;
      return `${m.sender} (${m.sender_kind})${isMention ? ' [DIRECT MENTION]' : ''}: ${text}`;
    }).join("\n");

    return {
      description: "Latest messages from the communication platform",
      messages: [
        {
          role: "user",
          content: {
            type: "text",
            text: `Here are the latest messages from the communication platform (#${channel}):\n\n${formatted}\n\nPlease respond to any messages that require your attention as the 'agent'.`
          }
        }
      ]
    };
  }
  throw new Error(`Prompt not found: ${request.params.name}`);
});

async function startMessagePoll() {
  log(`[Debug] Starting general message poll (every 10s)`);
  setInterval(async () => {
    try {
      const result = await apiRequest(`/api/messages?channel=general&since=${lastSeenTimestamp + 1}&limit=10&sort_order=asc`);
      if (result.messages.length > 0) {
        log(`[Debug] Found ${result.messages.length} new general messages`);
        for (const msg of result.messages) {
          lastSeenTimestamp = Math.max(lastSeenTimestamp, msg.created_at);
          const isMention = msg.sender_kind !== 'agent' && msg.text.startsWith(MENTION_PREFIX);
          if (isMention) {
            log(`[Mention Detected] From: ${msg.sender}, Content: ${msg.text}`);
            const cleanedText = msg.text.replace(MENTION_PREFIX, "").trim();

            // This is the "hook" that pushes the message as if typed in a terminal
            // We don't await this so the poller can keep running, 
            // but we handle the lifecycle inside the promise
            (async () => {
              let turnResult = { status: "unknown", content: "" };
              try {
                log(`[Sampling] Pushing mention to agent: "${cleanedText}"`);
                const samplingResult = await server.request(
                  {
                    method: "sampling/createMessage",
                    params: {
                      messages: [
                        {
                          role: "user",
                          content: {
                            type: "text",
                            text: cleanedText,
                          },
                        },
                      ],
                      // Context for the agent to know it's responding to a specific user
                      systemPrompt: `You are responding to ${msg.sender} on the communication platform.`,
                      maxTokens: 1000,
                    },
                  },
                  CreateMessageRequestSchema
                );

                if (samplingResult && samplingResult.content && samplingResult.content.type === "text") {
                  log(`[Sampling] Agent responded. Posting back to channel...`);
                  const responseText = samplingResult.content.text;
                  await apiRequest("/api/messages", {
                    method: "POST",
                    body: JSON.stringify({
                      channel: "general",
                      text: responseText,
                      sender: "Gemini",
                      sender_kind: "agent",
                    }),
                  });
                  turnResult = { status: "success", content: responseText };
                }
              } catch (samplingErr) {
                log(`[Sampling Failed] ${samplingErr.message}`);
                turnResult = { status: "error", content: samplingErr.message };
              } finally {
                // Report back the actual result instead of a generic "End of Turn"
                const resultSummary = turnResult.status === "success" 
                  ? `--- Result: Success ---` 
                  : `--- Result: Failed (${turnResult.content}) ---`;
                
                log(`[Turn Hook] Reporting result: ${resultSummary}`);
                await sendSystemMessage(resultSummary, "general");
                
                // Detailed notification for the end-turn listener
                server.notification({
                  method: "notifications/turn_end",
                  params: { 
                    channel: "general", 
                    timestamp: new Date().toISOString(),
                    status: turnResult.status,
                    result: turnResult.content
                  }
                });
              }
            })();

            server.notification({ method: "notifications/prompts/list_changed" });
          }
        }
      }
    } catch (err) {
      log(`[Debug] General poll error: ${err.message}`);
    }
  }, 10000);
}

// async function startPresenceAndMentionPoll() {
//   console.error(`[Debug] Starting presence/mention poll (every 5s) as user: ${DIR_NAME}`);
//   setInterval(async () => {
//     try {
//       const result = await apiRequest(`/api/messages?channel=general&user=${encodeURIComponent(DIR_NAME)}&since=${lastMentionTimestamp + 1}&limit=10&sort_order=asc`);

//       if (result.messages.length > 0) {
//         console.error(`[Debug] Found ${result.messages.length} messages in mention poll`);
//         let hasNewMention = false;
//         for (const msg of result.messages) {
//           const isMention = msg.sender_kind !== 'agent' && msg.text.startsWith(MENTION_PREFIX);
//           if (isMention) {
//             hasNewMention = true;
//             const cleanedText = msg.text.replace(MENTION_PREFIX, "").trim();
//             console.error(`[Mention Detected] From: ${msg.sender}, Content: ${cleanedText}`);
//             // We no longer use sampling to force a response. 
//             // Instead, we just notify the client so it can check the prompt.
//           }
//           lastMentionTimestamp = Math.max(lastMentionTimestamp, msg.created_at);
//         }

//         if (hasNewMention) {
//           server.notification({
//             method: "notifications/prompts/list_changed",
//           });
//         }
//       }
//     } catch (err) {
//       console.error(`[Debug] Mention poll error: ${err.message}`);
//     }
//   }, 5000);
// }

async function main() {
  const transport = new StdioServerTransport();
  await server.connect(transport);

  log(`[Startup] Gemini Chat Server v1.1.1 started and connected`);

  // Send init message on startup
  await sendSystemMessage(`🚀 Gemini Chat Extension started (@${DIR_NAME})`);

  startMessagePoll();
  //startPresenceAndMentionPoll();
}

main().catch(console.error);
