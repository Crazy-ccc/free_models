export type PageKey = 'overview' | 'providers' | 'models' | 'apiKeys' | 'stats' | 'settings';

export interface Provider {
  id: number;
  name: string;
  base_url: string;
  created_time: string;
  last_updated: string;
}

export interface ProviderCredential {
  id: number;
  provider_id: number;
  name: string;
  api_key: string;
  account: string | null;
  password: string | null;
  priority: number;
  is_active: boolean;
  created_time: string;
  last_updated: string;
}

export interface Model {
  id: number;
  name: string;
  timeout: number;
  priority: number;
  is_active: boolean;
  context_length: number;
  created_time: string;
  last_updated: string;
}

export interface ApiKey {
  id: number;
  key_value: string;
  name: string;
  is_active: boolean;
  created_time: string;
  last_updated: string;
}

export interface Penalty {
  modelName: string;
  providerName: string;
  credentialId: number;
  remainingSecs: number;
}

export interface ServiceStatus {
  healthy: boolean;
  models: { total: number; active: number; inactive: number };
  providers: { total: number; active: number; inactive: number };
  apiKeys: { total: number; active: number; inactive: number };
  penalties: Penalty[];
}

export interface ProviderModelMap {
  id: number;
  model_id: number;
  provider_id: number;
  provider_model_id: string;
  is_active: boolean;
  priority: number;
  context_length: number | null;
  protocols: string;
  status: string;
  timeout: number | null;
  created_time: string;
  last_updated: string;
}

export interface TestCredentialResult {
  success: boolean;
  response_time_ms: number;
  model_id: string;
  provider_id: number;
  credential_id: number;
  error: string | null;
}

export interface UsageLogStatsResponse {
  items: UsageLogStatItem[];
  total: {
    requests: number;
    prompt_tokens: number;
    completion_tokens: number;
    total_tokens: number;
    avg_duration_ms: number;
  };
}

export interface UsageLogStatItem {
  dimension_name: string;
  requests: number;
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  cache_hit_tokens: number;
  cache_miss_tokens: number;
  avg_duration_ms: number;
  max_duration_ms: number;
}
