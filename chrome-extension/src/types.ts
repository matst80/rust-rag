export type Tab = 'search' | 'chat' | 'store';

export interface Config {
  apiBaseUrl: string;
  apiToken: string | null;
}

export interface SearchResult {
  text: string;
  source_id: string;
  score?: number | null;
}

export interface AssistedResult {
  id: string;
  text: string;
  source_id: string;
  distance: number;
}

export type AssistedEvent =
  | { object: 'assisted_query.queries'; queries: string[] }
  | { object: 'assisted_query.result'; query: string; index: number; results: AssistedResult[] }
  | { object: 'assisted_query.merged'; results: AssistedResult[] };

export interface ChatMessage {
  id: string;
  role: 'user' | 'ai';
  content: string;
  thinking: string;
}

export interface PollingStatus {
  isPolling: boolean;
  userCode: string | null;
  verificationUri: string | null;
  verificationUriComplete: string | null;
}
